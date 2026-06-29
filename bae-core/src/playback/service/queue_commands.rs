use super::*;

impl PlaybackService {
    /// Emit queue update to all subscribers
    pub(super) async fn on_queue_mutated(&mut self) {
        self.pending_side_pause = None;
        self.refresh_preload_for_queue_front().await;
        self.emit_queue_update();
        self.persist_playback_state().await;
    }

    pub(super) fn emit_queue_update(&self) {
        let has_next = self.playback_queue.has_upcoming()
            || self.playback_queue.repeat_mode() != RepeatMode::Off;
        let has_previous = self.playback_queue.has_previous();
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::QueueUpdated {
                manual: self.playback_queue.manual_entries(),
                context: self.playback_queue.context_projection(),
                has_next,
                has_previous,
            },
        );
    }

    pub(super) fn emit_queue_items_added(&self, count: u32) {
        if count == 0 {
            return;
        }
        let event = PlaybackProgress::QueueItemsAdded { count };
        let _ = self.progress_tx.send(event);
    }
}
