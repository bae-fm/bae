use super::*;

impl PlaybackService {
    /// Compute the display values for `position_ms` on the current track,
    /// write them to `last_position_display`, and emit a `Seeked` progress event.
    ///
    /// This is the single sink for non-tick position updates (seek, restore,
    /// pause/resume refresh). Writing the Arc without emitting would leave the
    /// NSView stale; emitting without writing the Arc would leave late-mounting
    /// views without a cached value to query. Always go through this helper.
    pub(super) fn emit_position_display(&self, position_ms: u64, track_id: String) {
        let Some(prepared) = &self.current_prepared else {
            return;
        };
        let raw_dur_ms = prepared.duration.as_millis() as u64;
        let pregap_ms = prepared.pregap_ms;
        let (adjusted_pos_ms, adjusted_dur_ms) =
            crate::playback::format::adjust_for_pregap(position_ms, raw_dur_ms, pregap_ms);
        let progress =
            crate::playback::format::compute_progress(position_ms, raw_dur_ms, pregap_ms);

        *self.last_position_display.lock().unwrap() = Some(progress);

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::Seeked {
                position_ms: adjusted_pos_ms,
                duration_ms: adjusted_dur_ms,
                track_id,
                progress,
            },
        );
    }

    /// The fields the Playing and Paused states share, read from the current
    /// prepared track. Position data is excluded — it flows through
    /// PositionUpdate/Seeked events.
    pub(super) fn current_state_fields(&self) -> (PlaybackTrackInfo, u64) {
        let prepared = self.current_prepared.as_ref().expect("no current_prepared");
        (
            prepared.track_info.clone(),
            pregap_adjusted_duration(prepared),
        )
    }

    /// Build a Playing state from the current prepared track and track info.
    pub(super) fn make_playing_state(&self) -> PlaybackState {
        let (track_info, duration_ms) = self.current_state_fields();
        PlaybackState::Playing {
            track_info,
            duration_ms,
        }
    }

    /// Build a Paused state from the current prepared track and track info.
    pub(super) fn make_paused_state(&self, reason: PlaybackPauseReason) -> PlaybackState {
        let (track_info, duration_ms) = self.current_state_fields();
        PlaybackState::Paused {
            track_info,
            duration_ms,
            reason,
        }
    }

    /// Emit a `StateChanged` for the current track's play/pause state. Shared
    /// by the play, gapless-advance, and rebuild-advance paths.
    pub(super) fn emit_current_state(&self) {
        let state = if self.audio_output.is_paused() {
            self.make_paused_state(PlaybackPauseReason::Manual)
        } else {
            self.make_playing_state()
        };
        emit_progress(&self.progress_tx, PlaybackProgress::StateChanged { state });
    }

    /// Restore playback from the validated device-local resume cache.
    ///
    /// Atomic: every fallible step (fetching the context's tracks, validating the
    /// queue against library deletions) runs *before* anything touches the queue
    /// or audio. A DB error abandons the whole restore — the queue is left empty
    /// (a fresh start), never half-populated. Only once all fetches succeed does
    /// the commit run, and the commit is infallible — `parsed` is already
    /// fully-valid, so no field needs defaulting.
    ///
    /// StateChanged emissions are suppressed because the UI isn't ready yet;
    /// display state is written to the shared Arc for the UI to query later.
    pub(super) async fn restore(&mut self, parsed: PersistedPlayback) {
        info!(
            "Restoring playback state: track={:?}",
            parsed.queue.current_track_id
        );

        // -- All fallible work first; the queue is untouched until it succeeds. --

        // Re-materialize the context from its source's current tracks (deleted
        // tracks fall out of the re-fetch). A fetch error abandons the restore; an
        // empty result means the source is gone (the release was deleted, or the
        // library is now empty); a result shorter than the saved cursor means the
        // source shrank below where we were playing. Either way we drop the context
        // and restore the manual lane only, so `build_context` only ever sees an
        // in-range cursor.
        let (context, context_tracks) = match &parsed.queue.context {
            Some(cs) => match self.fetch_source_tracks(&cs.source).await {
                Ok(tracks) if tracks.is_empty() => {
                    debug!(
                        "resume context source {:?} is gone; restoring the manual lane only",
                        cs.source
                    );
                    (None, Vec::new())
                }
                Ok(tracks) if cs.cursor >= tracks.len() => {
                    warn!(
                        "saved cursor {} is past the {} current tracks of {:?}; \
                         restoring the manual lane only",
                        cs.cursor,
                        tracks.len(),
                        cs.source
                    );
                    (None, Vec::new())
                }
                Ok(tracks) => (parsed.queue.context, tracks),
                Err(e) => {
                    warn!(
                        "couldn't load the resume context tracks for {:?}: {e}; starting fresh",
                        cs.source
                    );
                    return;
                }
            },
            None => (None, Vec::new()),
        };

        // Drop manual-lane tracks and a current track that were deleted from the
        // library between sessions (deleted context tracks already fell out of the
        // re-fetch above). A validation error abandons the restore.
        let mut to_check = parsed.queue.manual.clone();
        to_check.extend(parsed.queue.current_track_id.clone());
        let existing = match self
            .library_manager
            .filter_existing_track_ids(&to_check)
            .await
        {
            Ok(existing) => existing,
            Err(e) => {
                warn!(
                    "couldn't validate restored tracks {to_check:?} against deletions: {e}; \
                     starting fresh"
                );
                return;
            }
        };
        let dropped: Vec<&String> = to_check.iter().filter(|t| !existing.contains(*t)).collect();
        if !dropped.is_empty() {
            warn!("dropping playback tracks deleted from the library: {dropped:?}");
        }
        let manual: Vec<String> = parsed
            .queue
            .manual
            .into_iter()
            .filter(|t| existing.contains(t))
            .collect();
        let current_track_id = parsed
            .queue
            .current_track_id
            .filter(|t| existing.contains(t));
        let repeat = parsed.queue.repeat;

        // -- Commit (infallible): everything below applies validated state. --

        self.playback_queue.restore(
            QueueSnapshot {
                context,
                manual,
                current_track_id,
                repeat,
            },
            context_tracks,
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged { mode: repeat },
        );

        // Volume + mute.
        self.audio_output.set_volume(parsed.volume);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::VolumeChanged {
                volume: parsed.volume,
            },
        );
        if parsed.is_muted {
            self.is_muted = true;
            self.pre_mute_volume = parsed.volume;
            self.audio_output.set_volume(0.0);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::MuteChanged { is_muted: true },
            );
        }

        self.emit_queue_update();

        // Start the current track paused at the saved position, if there is one.
        if let Some(track_id) = self
            .playback_queue
            .current_track_id()
            .map(|s| s.to_string())
        {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Paused);
            let start = parsed
                .position_ms
                .map(|pos| TrackStart::Position(std::time::Duration::from_millis(pos)))
                .unwrap_or(TrackStart::Direct);
            self.play_track(&track_id, start, true).await;

            // Emit the position we restored to so late-mounting views can read it
            // on mount. `None` means none was captured — the track's start (0).
            let restored_pos = parsed.position_ms.unwrap_or(0);
            self.emit_position_display(restored_pos, track_id);
        }

        // Write the reconciled state back: dropping a dead context or
        // library-deleted tracks corrected the in-memory queue, so persist makes
        // that correction durable now (or clears the row if nothing is playing)
        // rather than leaving the saved row stale until the next change.
        self.persist_playback_state().await;

        info!("Playback state restored");
    }

    /// Build the device-local `playback_state` row from the current queue and
    /// playback state, and save it — or clear it when playback has stopped. The
    /// queue is device-local; this never syncs.
    ///
    /// The write is logged-best-effort, not propagated as fatal: a failed
    /// resume-cache write only costs the resume point; playback is unaffected.
    /// Fetch a context's tracks in source order for the source it plays from:
    /// a release's ordered track ids, or every library track. The one place the
    /// service re-derives a context's tracks (the shuffle toggle, restore, and
    /// any future `Context`-repeat re-fetch dispatch here so the two sources stay
    /// in lockstep).
    pub(super) async fn fetch_source_tracks(
        &self,
        source: &ContextSource,
    ) -> Result<Vec<String>, crate::library::LibraryError> {
        match source {
            ContextSource::Release(id) => self.library_manager.get_track_ids(id).await,
            ContextSource::Library => self.library_manager.get_all_track_ids().await,
        }
    }

    /// The log is the never-mask escape hatch — a write failure is recorded, not
    /// conflated with "nothing was playing".
    pub(super) async fn persist_playback_state(&self) {
        use crate::playback::audio_output::AudioState;
        if self.audio_output.get_state() == AudioState::Stopped {
            if let Err(e) = self.library_manager.clear_playback_state().await {
                warn!("couldn't clear playback state: {e}");
            }
            return;
        }
        let snap = self.playback_queue.snapshot();
        let context = snap.context.map(|ctx| DbPlaybackContext {
            source: source_to_str(&ctx.source),
            shuffle_seed: match ctx.traversal {
                Traversal::Shuffled { seed } => Some(seed as i64),
                Traversal::Sequential => None,
            },
            cursor: ctx.cursor as i64,
        });
        let position_ms =
            (*self.current_position_shared.lock().unwrap()).map(|d| d.as_millis() as i64);
        let row = DbPlaybackState {
            context,
            manual: serde_json::to_string(&snap.manual)
                .expect("serializing a Vec<String> to JSON cannot fail"),
            repeat: repeat_to_str(snap.repeat),
            current_track_id: snap.current_track_id,
            position_ms,
            volume: if self.is_muted {
                self.pre_mute_volume
            } else {
                self.audio_output.get_volume()
            },
            is_muted: self.is_muted,
        };
        if let Err(e) = self.library_manager.save_playback_state(&row).await {
            warn!(
                "couldn't persist playback state (current track {:?}): {e}",
                row.current_track_id
            );
        }
    }

    pub(super) async fn pause(&mut self) {
        self.pending_side_pause = None;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Paused);
        if self.current_prepared.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_paused_state(PlaybackPauseReason::Manual),
                },
            );
        }
    }

    pub(super) async fn resume(&mut self) {
        // Stop preview when user explicitly resumes main playback
        if self.preview.is_active() {
            self.main_was_playing_before_preview = false;
            self.preview.stop();
        }

        if self.pending_side_pause.is_some() {
            self.resume_from_side_pause().await;
            return;
        }

        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Playing);
        if self.current_prepared.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_playing_state(),
                },
            );
        }
    }
}
