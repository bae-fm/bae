//! A serial release queue and the snapshot stream its pane reads, as one
//! owner. Every change to the queue republishes the snapshot, so the pane
//! can never see a queue the stream has not followed.

use crate::library::release_queue::{run_serial_worker, ReleaseQueue, ReleaseQueueOp, RunningOp};
use crate::library::LibraryError;
use std::future::Future;
use std::sync::Arc;

pub struct QueuedReleases<Extra, Progress, Snapshot> {
    queue: Arc<ReleaseQueue<Extra, Progress>>,
    values: tokio::sync::watch::Sender<Snapshot>,
    /// What the pane draws, built from the queue's rows and its paused flag.
    build: fn(&[ReleaseQueueOp<Extra, Progress>], bool) -> Snapshot,
}

impl<Extra, Progress, Snapshot> Clone for QueuedReleases<Extra, Progress, Snapshot> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            values: self.values.clone(),
            build: self.build,
        }
    }
}

impl<Extra: Clone, Progress: Clone, Snapshot> QueuedReleases<Extra, Progress, Snapshot> {
    pub fn new(build: fn(&[ReleaseQueueOp<Extra, Progress>], bool) -> Snapshot) -> Self {
        let (values, _) = tokio::sync::watch::channel(build(&[], false));
        Self {
            queue: Arc::new(ReleaseQueue::new()),
            values,
            build,
        }
    }

    pub fn contains(&self, release_id: &str) -> bool {
        self.queue.contains(release_id)
    }

    /// Enqueue every row, wake the worker if any landed, and publish once.
    /// Returns whether any did.
    pub fn enqueue_all(
        &self,
        ops: impl IntoIterator<Item = ReleaseQueueOp<Extra, Progress>>,
    ) -> bool {
        let mut added = false;
        for op in ops {
            added |= self.queue.enqueue(op);
        }
        if added {
            self.queue.wake();
            self.republish();
        }
        added
    }

    /// Pause or resume. While paused the worker parks instead of starting
    /// the next release; the in-flight one runs to completion. Resuming
    /// wakes the worker.
    pub fn set_paused(&self, paused: bool) {
        let was_paused = self.queue.set_paused(paused);
        if was_paused && !paused {
            self.queue.wake();
        }
        self.republish();
    }

    /// Drop a queued or failed entry; abort the active one's task.
    pub fn cancel(&self, release_id: &str) {
        self.queue.cancel(release_id);
        self.republish();
    }

    /// Flip every failed entry back to queued and wake the worker.
    pub fn retry_failed(&self) {
        if self.queue.retry_failed() {
            self.queue.wake();
        }
        self.republish();
    }

    /// Record the active entry's progress. `false` when no active entry has
    /// that id, in which case nothing is published.
    pub fn set_active_progress(&self, release_id: &str, progress: Progress) -> bool {
        let set = self.queue.set_active_progress(release_id, progress);
        if set {
            self.republish();
        }
        set
    }

    pub fn snapshot(&self) -> Snapshot {
        (self.build)(&self.queue.ops(), self.queue.is_paused())
    }

    pub fn republish(&self) {
        self.values.send_replace(self.snapshot());
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Snapshot> {
        self.values.subscribe()
    }

    /// Drain the queue one release at a time — see
    /// [`run_serial_worker`] for the protocol — publishing after every
    /// change the worker makes.
    pub async fn run_serial<Start, StartFut, Fut, Done>(
        &self,
        label: &'static str,
        start: Start,
        on_done: Done,
    ) where
        Start: FnMut(ReleaseQueueOp<Extra, Progress>) -> StartFut,
        StartFut: Future<Output = Result<(Progress, RunningOp<Fut>), LibraryError>>,
        Fut: Future<Output = Result<(), LibraryError>>,
        Done: FnMut(&str, Result<(), &LibraryError>),
    {
        run_serial_worker(&self.queue, label, start, || self.republish(), on_done).await
    }

    /// Mark an entry active with the task that runs it, as the worker does —
    /// for a test that drives progress without a worker.
    #[cfg(test)]
    pub fn activate(
        &self,
        release_id: &str,
        abort: tokio::task::AbortHandle,
        progress: Progress,
    ) -> bool {
        let activated = self.queue.activate(release_id, abort, progress);
        self.republish();
        activated
    }
}

impl<Extra: Clone, Snapshot> QueuedReleases<Extra, u8, Snapshot> {
    /// Record the active entry's percent, when it is still queued.
    pub fn set_active_percent(&self, release_id: &str, percent: u8) {
        if self.queue.contains(release_id) {
            self.queue.set_active_percent(release_id, percent);
            self.republish();
        }
    }
}
