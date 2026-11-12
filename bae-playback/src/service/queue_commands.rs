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
    pub(super) async fn queue_added(
        &mut self,
        added: Result<usize, bae_store::DbError>,
    ) -> Result<(), QueueEditError> {
        match added {
            // The write counted the addition on the queue row, which the
            // screens confirming it read.
            Ok(_) => {
                self.on_queue_mutated().await;
                Ok(())
            }
            Err(e) => Err(self.queue_add_failed("add tracks to the queue", e)),
        }
    }

    /// Put a release's tracks into Up Next at `place`. A release with no
    /// tracks has nothing to put in.
    pub(super) async fn add_release(
        &mut self,
        release_id: &str,
        place: ManualPlace,
    ) -> Result<(), QueueEditError> {
        let track_ids = match self.records.get_track_ids_for_release(release_id).await {
            Ok(track_ids) => track_ids,
            Err(e) => {
                return Err(self.queue_add_failed(&format!("read release {release_id}'s tracks"), e))
            }
        };
        if track_ids.is_empty() {
            return Ok(());
        }
        let added = match place {
            ManualPlace::Front => self.playback_queue.add_next(track_ids).await,
            ManualPlace::End => self.playback_queue.add_to_queue(track_ids).await,
            ManualPlace::At(index) => self.playback_queue.insert_at(index, track_ids).await,
        };
        self.queue_added(added).await
    }

    /// An add to Up Next failed: the queue is as it was.
    fn queue_add_failed(&self, doing: &str, error: bae_store::DbError) -> QueueEditError {
        error!("couldn't {doing}: {error}");
        self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
        QueueEditError::Queue(error)
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
                self.queue_failed_halting("move the queue on", e);
                NextEntry::Stop
            }
        }
    }

    pub(super) async fn apply_repeat_mode(
        &mut self,
        mode: RepeatMode,
    ) -> Result<(), QueueEditError> {
        if self.playback_queue.repeat_mode() == mode {
            return Ok(());
        }
        if let Err(e) = self.playback_queue.set_repeat_mode(mode).await {
            self.queue_failed("set the repeat mode", &e);
            return Err(QueueEditError::Queue(e));
        }
        self.persist_playback_state().await;
        Ok(())
    }
}
