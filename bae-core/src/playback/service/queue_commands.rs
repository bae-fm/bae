use super::*;

impl PlaybackService {
    /// The common tail of every queue mutation: reconcile the side-pause and the
    /// preload against the new queue, and persist. The queue published itself
    /// when it changed — `PublishedQueue::apply` does that — so nothing here
    /// emits it.
    pub(super) async fn on_queue_mutated(&mut self) {
        // The mutation may have invalidated which track a side-pause resumes into,
        // so forget the side-pause (demote to a plain manual pause) without
        // emitting — the UI keeps showing the paused state it last saw.
        self.demote_side_pause_to_manual();
        self.refresh_preload_for_queue_front().await;
        self.persist_playback_state().await;
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
