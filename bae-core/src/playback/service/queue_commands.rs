use super::*;

impl PlaybackService {
    /// The common tail of every queue mutation: reconcile the side-pause and the
    /// preload against the new queue, emit it, and persist.
    pub(super) async fn on_queue_mutated(&mut self) {
        // The mutation may have invalidated which track a side-pause resumes into,
        // so forget the side-pause (demote to a plain manual pause) without
        // emitting — the UI keeps showing the paused state it last saw.
        self.demote_side_pause_to_manual();
        self.refresh_preload_for_queue_front().await;
        self.emit_queue_update();
        self.persist_playback_state().await;
    }

    pub(super) fn emit_queue_update(&self) {
        let projection = self.queue_projection();
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::QueueUpdated(projection),
        );
    }

    pub(super) fn queue_projection(&self) -> PlaybackQueueProjection {
        PlaybackQueueProjection {
            manual: self.playback_queue.manual_entries(),
            context: self.playback_queue.context_projection(),
            has_next: self.playback_queue.has_upcoming()
                || self.playback_queue.repeat_mode() != RepeatMode::Off,
            has_previous: self.playback_queue.has_previous(),
            revision: self.playback_queue.revision(),
        }
    }

    pub(super) fn emit_queue_items_added(&self, count: u32) {
        if count == 0 {
            return;
        }
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::QueueItemsAdded { count },
        );
    }
}
