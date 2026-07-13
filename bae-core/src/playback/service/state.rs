use super::*;

impl PlaybackService {
    /// The current track's id, once one exists (Active in any phase). None while
    /// the slot is Stopped or still resolving a fresh load.
    pub(super) fn current_track_id(&self) -> Option<&str> {
        self.slot.current_track_id()
    }

    /// Compute the display values for `position_ms` on the current track and
    /// emit a `Seeked` progress event. The single emitter for non-tick position
    /// updates (seek, restore, pause/resume refresh).
    pub(super) fn emit_position_display(&self, position_ms: u64, track_id: String) {
        let PlaybackSlot::Active(cur) = &self.slot else {
            return;
        };
        let prepared = &cur.prepared;
        let raw_dur_ms = prepared.duration.as_millis() as u64;
        let pregap_ms = prepared.total_pregap_ms();
        let (adjusted_pos_ms, adjusted_dur_ms) =
            crate::playback::format::adjust_for_pregap(position_ms, raw_dur_ms, pregap_ms);
        let progress =
            crate::playback::format::compute_progress(position_ms, raw_dur_ms, pregap_ms);

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

    /// Pure map of the current slot to the public `PlaybackState`. Position data
    /// is excluded — it flows through `PositionUpdate`/`Seeked` events.
    pub(super) fn playback_state(&self) -> PlaybackState {
        match &self.slot {
            PlaybackSlot::Stopped => PlaybackState::Stopped,
            PlaybackSlot::Loading { track_id, resolved } => PlaybackState::Loading {
                track_id: track_id.clone(),
                resolved: resolved.clone(),
            },
            PlaybackSlot::Active(cur) => match &cur.phase {
                TrackPhase::Loading { .. } => PlaybackState::Loading {
                    track_id: cur.prepared.track_info.track_id.clone(),
                    resolved: Some(LoadingTrack::from_prepared(&cur.prepared)),
                },
                TrackPhase::Playing => PlaybackState::Playing {
                    track_info: cur.prepared.track_info.clone(),
                    duration_ms: pregap_adjusted_duration(&cur.prepared),
                },
                TrackPhase::Paused(pause) => PlaybackState::Paused {
                    track_info: cur.prepared.track_info.clone(),
                    duration_ms: pregap_adjusted_duration(&cur.prepared),
                    reason: pause.to_reason(),
                },
                // A Completed track emits no public state — the machine leaves
                // Completed via AutoAdvance / stop / side-pause, each emitting its
                // own terminal state. This arm is the assertion that nothing calls
                // `emit_state` while Completed.
                TrackPhase::Completed => {
                    unreachable!("Completed is never emitted as a public state")
                }
            },
        }
    }

    /// The ONLY place a `StateChanged` is emitted. Every transition mutates the
    /// slot (and syncs the atomic), then calls this. Emitting while the current
    /// track is Completed is a bug — `playback_state` treats it as unreachable.
    pub(super) fn emit_state(&self) {
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: self.playback_state(),
            },
        );
    }

    /// Write the shared `AudioState` atomic as a projection of the slot. The
    /// atomic is the realtime plane the audio callback reads lock-free every
    /// buffer; the slot is the truth. Call after every slot/phase mutation.
    pub(super) fn sync_audio_state(&self) {
        use crate::playback::audio_output::AudioState;
        let projected = match &self.slot {
            PlaybackSlot::Stopped => AudioState::Stopped,
            // A load is in flight. The output stream persists across the
            // transition, so its callback keeps running — project silence until
            // the load's track attaches and install runs this again with the
            // resolved intent. This is what keeps the old ring from leaking audio
            // during play_track's swap and seek's rebuild.
            PlaybackSlot::Loading { .. } => AudioState::Stopped,
            PlaybackSlot::Active(cur) => match cur.phase.intent() {
                PlayIntent::Playing => AudioState::Playing,
                PlayIntent::Paused => AudioState::Paused,
                PlayIntent::Stopped => AudioState::Stopped,
            },
        };
        self.audio_output.set_state(projected);
    }

    /// The play/pause intent to carry to a track we switch to (Next / Previous /
    /// restart). A side-pause is meaningless for a different track, so it
    /// collapses to a plain manual pause; Completed carries Playing.
    pub(super) fn current_play_target(&self) -> PlayTarget {
        match &self.slot {
            PlaybackSlot::Active(cur) => match &cur.phase {
                TrackPhase::Playing | TrackPhase::Completed => PlayTarget::Playing,
                TrackPhase::Paused(_) => PlayTarget::Paused(PausePhase::Manual),
                TrackPhase::Loading { target, .. } => match target {
                    PlayTarget::Playing => PlayTarget::Playing,
                    PlayTarget::Paused(_) => PlayTarget::Paused(PausePhase::Manual),
                },
            },
            _ => PlayTarget::Playing,
        }
    }

    /// Whether the current track is paused at a physical-side boundary.
    pub(super) fn is_side_paused(&self) -> bool {
        matches!(
            &self.slot,
            PlaybackSlot::Active(cur)
                if matches!(cur.phase, TrackPhase::Paused(PausePhase::SideEnded(_)))
        )
    }

    /// Resolve a load's `TrackReady`: if it matches the current Loading phase's
    /// generation, advance the phase to that load's target (Playing/Paused) and
    /// emit. A stale generation (a superseded load, or a pause/preview that
    /// collapsed the Loading phase) is dropped.
    pub(super) fn resolve_track_ready(&mut self, track_id: String, generation: LoadGeneration) {
        let resolved = match &mut self.slot {
            PlaybackSlot::Active(cur) => match &cur.phase {
                TrackPhase::Loading {
                    generation: live, ..
                } if *live == generation => {
                    if let TrackPhase::Loading { target, .. } =
                        std::mem::replace(&mut cur.phase, TrackPhase::Playing)
                    {
                        cur.phase = target.into_track_phase();
                    }
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if resolved {
            // This load reached Playing: if it's the one we timed, ship the
            // time-to-first-audio and clear the measurement.
            if self
                .first_audio_pending
                .as_ref()
                .is_some_and(|m| m.generation == generation)
            {
                let measurement = self.first_audio_pending.take().expect("just checked Some");
                self.telemetry().event(TelemetryEvent::FirstAudio {
                    track_id: LocalId(measurement.track_id),
                    wait: measurement.started_at.elapsed(),
                });
            }
            self.sync_audio_state();
            self.emit_state();
        } else {
            debug!(track_id, "ignoring stale TrackReady from an abandoned load");
        }
    }

    /// Restore playback from the validated device-local resume cache.
    ///
    /// Atomic: every fallible step (fetching the context's tracks, validating the
    /// queue against library deletions) runs *before* anything touches the queue
    /// or audio. A DB error abandons the whole restore — the queue is left empty
    /// (a fresh start), never half-populated. Only once all fetches succeed does
    /// the commit run, and the commit is infallible — `parsed` is already
    /// fully-valid, so no field needs defaulting.
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

        let restored_context_track_count = context_tracks.len();
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
            let start = parsed
                .position_ms
                .map(|pos| TrackStart::Position(std::time::Duration::from_millis(pos)))
                .unwrap_or(TrackStart::Direct);
            // Paused target: `play_track` ships no `TrackStarted`, so the
            // transition here is unused — a manual restore into pause.
            self.play_track(
                &track_id,
                start,
                PlayTarget::Paused(PausePhase::Manual),
                TrackTransition::Manual,
            )
            .await;

            // Emit the restored position as a `Seeked` so subscribers position their
            // display. No saved position means the track's start.
            let restored_pos = parsed.position_ms.unwrap_or(0);
            self.emit_position_display(restored_pos, track_id);
        }

        // Dropping a dead context or library-deleted tracks corrected the in-memory
        // queue: make that correction durable now (or clear the row if nothing is
        // playing) rather than leaving the saved row stale until the next change.
        self.persist_playback_state().await;

        info!("Playback state restored");
        self.telemetry_playback_started(
            PlaybackStartSource::Restored,
            restored_context_track_count,
        );
    }

    /// Fetch a context's tracks in source order for the source it plays from:
    /// a release's ordered track ids, several releases' track ids concatenated in
    /// input order, or every library track. The one place the service re-derives a
    /// context's tracks (the shuffle toggle, restore, and any future
    /// `Context`-repeat re-fetch dispatch here so the sources stay in lockstep). A
    /// missing release fails the whole re-fetch — the caller leaves the queue
    /// unchanged rather than silently dropping tracks from a live order.
    pub(super) async fn fetch_source_tracks(
        &self,
        source: &ContextSource,
    ) -> Result<Vec<String>, crate::library::LibraryError> {
        match source {
            ContextSource::Release(id) => self.library_manager.get_track_ids(id).await,
            ContextSource::Releases(ids) => {
                let mut track_ids = Vec::new();
                for id in ids {
                    track_ids.extend(self.library_manager.get_track_ids(id).await?);
                }
                Ok(track_ids)
            }
            ContextSource::Library => self.library_manager.get_all_track_ids().await,
        }
    }

    /// Load the tracks for a set of releases in input order, concatenated,
    /// skipping (with a log) any release whose tracks fail to load or that has
    /// none — the per-release form of `PlayRelease`'s log-and-continue, so one bad
    /// album doesn't sink the rest of a multi-album play. Returns the releases that
    /// contributed and their concatenated track ids; an empty track list means
    /// nothing was playable.
    pub(super) async fn load_release_set_tracks(
        &self,
        release_ids: Vec<String>,
    ) -> (Vec<String>, Vec<String>) {
        let mut playable_ids = Vec::new();
        let mut track_ids = Vec::new();
        for release_id in release_ids {
            match self.library_manager.get_track_ids(&release_id).await {
                Ok(ids) if ids.is_empty() => {
                    warn!("PlayReleases: release {release_id} has no tracks; skipping");
                }
                Ok(ids) => {
                    playable_ids.push(release_id);
                    track_ids.extend(ids);
                }
                Err(e) => {
                    warn!(
                        "PlayReleases: couldn't load tracks for release {release_id}: {e}; skipping"
                    );
                }
            }
        }
        (playable_ids, track_ids)
    }

    /// Build the device-local `playback_state` row from the current queue and
    /// playback state, and save it — or clear it when playback has stopped. The
    /// queue is device-local; this never syncs.
    ///
    /// The write is logged-best-effort, not propagated as fatal: a failed
    /// resume-cache write only costs the resume point; playback is unaffected.
    /// The log is the never-mask escape hatch — a write failure is recorded, not
    /// conflated with "nothing was playing".
    pub(super) async fn persist_playback_state(&mut self) {
        // Every call writes (or clears) the row, so it resets the per-tick throttle
        // in `handle_position_event` — including the persist a track change already
        // does, which is why a new track's first periodic write waits a full second
        // rather than firing at once.
        self.last_position_persist = Some(std::time::Instant::now());
        // Nothing to resume: the slot is Stopped, or the track drained naturally
        // (Completed). A Loading or playing/paused Active track writes the row.
        let nothing_to_resume = matches!(&self.slot, PlaybackSlot::Stopped)
            || matches!(
                &self.slot,
                PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Completed)
            );
        if nothing_to_resume {
            if let Err(e) = self.library_manager.clear_playback_state().await {
                warn!("couldn't clear playback state: {e}");
                self.telemetry_anomaly(AnomalyKind::ResumePersistFailed);
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
            self.telemetry_anomaly(AnomalyKind::ResumePersistFailed);
        }
    }

    pub(super) fn pause(&mut self) {
        // Pausing during a load collapses the Loading phase to Paused, so the
        // pending TrackReady no longer matches and is ignored. A Stopped or
        // still-resolving slot is a no-op (nothing is playing to pause).
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Paused(PausePhase::Manual);
            self.sync_audio_state();
            self.emit_state();
        }
    }

    pub(super) async fn resume(&mut self) {
        // An explicit resume of the main player dismisses any preview.
        if self.preview.is_active() {
            self.stop_preview_for_main_playback();
        }

        if self.is_side_paused() {
            self.resume_from_side_pause().await;
            return;
        }

        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Playing;
            self.sync_audio_state();
            self.emit_state();
        }
    }
}
