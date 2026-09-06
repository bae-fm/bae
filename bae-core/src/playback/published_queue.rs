//! The playback queue and the projection stream its UIs read, as one owner.
//!
//! Every change to the queue goes through [`PublishedQueue::apply`], which
//! republishes the projection in the same call. There is no way to mutate the
//! queue without the stream following it, so no reader can see a queue the
//! stream never carried.

use coven::IdRef;

use super::{PlaybackQueue, PlaybackQueueProjection, QueueSnapshot, RepeatMode};

pub struct PublishedQueue {
    queue: PlaybackQueue,
    values: tokio::sync::watch::Sender<PlaybackQueueProjection>,
}

impl PublishedQueue {
    pub fn new(ids: IdRef) -> Self {
        let queue = PlaybackQueue::new(ids);
        let (values, _) = tokio::sync::watch::channel(PlaybackQueueProjection::from_queue(&queue));
        Self { queue, values }
    }

    /// Change the queue, and republish when the change moved it. The queue's
    /// revision decides that: it bumps on exactly the mutations that change
    /// what the projection carries, so a call that changed nothing — an entry
    /// id that names no entry, a repeat mode already set — publishes nothing,
    /// and costs none of the database work each publish sets off downstream.
    /// Whatever the change computed comes back to the caller.
    pub fn apply<R>(&mut self, change: impl FnOnce(&mut PlaybackQueue) -> R) -> R {
        let before = self.queue.revision();
        let result = change(&mut self.queue);
        if self.queue.revision() != before {
            self.values
                .send_replace(PlaybackQueueProjection::from_queue(&self.queue));
        }
        result
    }

    /// The queue as the UIs read it, for a caller answering a one-shot request
    /// rather than following the stream.
    pub fn projection(&self) -> PlaybackQueueProjection {
        PlaybackQueueProjection::from_queue(&self.queue)
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<PlaybackQueueProjection> {
        self.values.subscribe()
    }

    // -- Reads, forwarded to the queue -----------------------------------------

    /// See [`PlaybackQueue::revision`].
    pub fn revision(&self) -> u64 {
        self.queue.revision()
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.queue.repeat_mode()
    }

    /// See [`PlaybackQueue::front`].
    pub fn front(&self) -> Option<&str> {
        self.queue.front()
    }

    pub fn current_track_id(&self) -> Option<&str> {
        self.queue.current_track_id()
    }

    /// See [`PlaybackQueue::next_sequential_context_track`].
    pub fn next_sequential_context_track(&self) -> Option<&str> {
        self.queue.next_sequential_context_track()
    }

    /// The persistable view of the queue, for the `playback_state` row. Distinct
    /// from [`PublishedQueue::projection`], which is what the UIs render.
    pub fn snapshot(&self) -> QueueSnapshot {
        self.queue.snapshot()
    }
}

impl PlaybackQueueProjection {
    fn from_queue(queue: &PlaybackQueue) -> Self {
        Self {
            manual: queue.manual_entries(),
            context: queue.context_projection(),
            has_next: queue.has_upcoming() || queue.repeat_mode() != RepeatMode::Off,
            has_previous: queue.has_previous(),
            revision: queue.revision(),
        }
    }
}
