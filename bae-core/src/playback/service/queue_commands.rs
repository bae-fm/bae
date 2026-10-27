use super::*;

impl PlaybackService {
    /// The common tail of every queue mutation: reconcile the side-pause and the
    /// preload against the new queue, and persist. The queue screens read the
    /// tables the mutation wrote, so nothing here emits the queue.
    pub(super) async fn on_queue_mutated(&mut self) {
        // The mutation may have invalidated which track a side-pause resumes into,
        // so forget the side-pause (demote to a plain manual pause) without
        // emitting — the UI keeps showing the paused state it last saw.
        self.demote_side_pause_to_manual();
        self.refresh_preload_for_queue_front().await;
        self.persist_playback_state().await;
    }

    /// Tracks went into Up Next, or the write failed.
    pub(super) async fn queue_added(&mut self, added: Result<usize, crate::db::DbError>) {
        match added {
            Ok(count) => {
                self.emit_queue_items_added(count as u32);
                self.on_queue_mutated().await;
            }
            Err(e) => {
                error!("couldn't add tracks to the queue: {e}");
                self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
            }
        }
    }

    /// A play filled the context lane: play its first track.
    pub(super) async fn start_filled_context(
        &mut self,
        first_track: String,
        source: PlaybackStartSource,
        track_count: usize,
    ) {
        self.stop_preview_for_main_playback();
        self.telemetry_playback_started(source, track_count);
        self.play_track(
            &first_track,
            TrackStart::Direct,
            PlayTarget::Playing,
            TrackTransition::Manual,
        )
        .await;
    }

    /// What the queue plays next. A failed queue read or write stops there:
    /// nothing is known to follow.
    pub(super) async fn next_entry(&mut self) -> NextEntry {
        match self.playback_queue.next_entry().await {
            Ok(next) => next,
            Err(e) => {
                self.queue_failed("move the queue on", e);
                NextEntry::Stop
            }
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

    pub(super) async fn apply_repeat_mode(&mut self, mode: RepeatMode) {
        if self.playback_queue.repeat_mode() == mode {
            return;
        }
        if let Err(e) = self.playback_queue.set_repeat_mode(mode).await {
            self.queue_failed("set the repeat mode", e);
            return;
        }
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged { mode },
        );
        self.persist_playback_state().await;
    }
}
