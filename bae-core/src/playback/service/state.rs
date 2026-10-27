use super::*;

impl PlaybackService {
    /// The current track's id, once one exists (Active in any phase). None while
    /// the slot is Stopped or still resolving a fresh load.
    pub(super) fn current_track_id(&self) -> Option<&str> {
        self.slot.current_track_id()
    }

    /// The current track and where its stream has played to; `None` without
    /// a current track.
    pub(super) fn current_track_position(&self) -> Option<(String, StreamPosition)> {
        match &self.slot {
            PlaybackSlot::Active(cur) => Some((cur.prepared.track_id.clone(), cur.position)),
            _ => None,
        }
    }

    /// How far the current track has played from its start (INDEX 01): the
    /// time the player shows, negative while the pregap plays. `None` without a
    /// current track.
    pub(super) fn current_track_time(&self) -> Option<TrackTime> {
        match &self.slot {
            PlaybackSlot::Active(cur) => Some(cur.prepared.timeline.track_time(cur.position)),
            _ => None,
        }
    }

    /// Emit the current track's position as a `Seeked` progress event: the
    /// single emitter for non-tick position updates (a seek, a paused load).
    /// Nothing without a current track.
    pub(super) fn emit_position_display(&self) {
        let PlaybackSlot::Active(cur) = &self.slot else {
            return;
        };
        let timeline = cur.prepared.timeline;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::Seeked {
                position_ms: timeline.track_time(cur.position).as_millis(),
                duration_ms: timeline.duration_ms(),
                track_id: cur.prepared.track_id.clone(),
                progress: timeline.progress(cur.position),
            },
        );
    }

    /// Pure map of the current slot to the public `PlaybackState`. Position data
    /// is excluded — it flows through `PositionUpdate`/`Seeked` events.
    pub(super) fn playback_state(&self) -> PlaybackState {
        match &self.slot {
            PlaybackSlot::Stopped => PlaybackState::Stopped,
            PlaybackSlot::Loading(track) => PlaybackState::Loading {
                track: track.clone(),
            },
            PlaybackSlot::Active(cur) => match &cur.phase {
                TrackPhase::Loading { .. } => PlaybackState::Loading {
                    track: LoadingTrack::Prepared(PlayingTrack::from_prepared(&cur.prepared)),
                },
                TrackPhase::Playing => PlaybackState::Playing {
                    track: PlayingTrack::from_prepared(&cur.prepared),
                },
                TrackPhase::Paused(pause) => PlaybackState::Paused {
                    track: PlayingTrack::from_prepared(&cur.prepared),
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
            PlaybackSlot::Loading(_) => AudioState::Stopped,
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
    pub(super) async fn resolve_track_ready(
        &mut self,
        track_id: String,
        generation: LoadGeneration,
    ) {
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
            self.renderer.reanchor_airplay();
            if !self.begin_listening().await {
                self.stop().await;
                return;
            }
            // This load reached Playing: if it's the one we timed, ship the
            // time-to-first-audio and clear the measurement.
            if self
                .first_audio_pending
                .as_ref()
                .is_some_and(|m| m.generation == generation)
            {
                let measurement = self.first_audio_pending.take().expect("just checked Some");
                self.record_telemetry(TelemetryEvent::FirstAudio {
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

    /// Resume playback from the device-local resume row over the queue the
    /// tables hold: the repeat mode, the volume, and the entry playing, loaded
    /// paused at its saved position. A deleted track left no entry to resume.
    /// A failed read of the queue resumes nothing.
    pub(super) async fn restore(&mut self, state: DbPlaybackState) {
        info!("Restoring playback state: entry={:?}", state.current);
        if let Err(e) = self.playback_queue.restore(state.current).await {
            warn!("couldn't read the queue to resume: {e}; resuming nothing");
            return;
        }
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged {
                mode: self.playback_queue.repeat_mode(),
            },
        );

        // The saved level is what the UI shows even when the session was muted —
        // that is the level unmuting puts back — so `VolumeChanged` carries it
        // either way, and the mute is announced on top.
        self.volume
            .restore(state.volume, state.is_muted, self.audio_output.as_ref());
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::VolumeChanged {
                volume: state.volume,
            },
        );
        if state.is_muted {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::MuteChanged { is_muted: true },
            );
        }

        // Start the current track paused at the saved position, or at its start
        // without one; loading it paused shows that position.
        if let Some(track_id) = self.playback_queue.current_track_id().map(str::to_string) {
            let start = state
                .position_ms
                .map(|position| TrackStart::Position(StreamPosition::from_millis(position)))
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
        }

        // A resume with nothing left to play clears the row now rather than
        // leaving it until the next change.
        self.persist_playback_state().await;

        info!("Playback state restored");
        match self.playback_queue.context_track_count().await {
            Ok(count) => self.telemetry_playback_started(PlaybackStartSource::Restored, count),
            Err(e) => warn!("couldn't count the resumed context lane: {e}"),
        }
    }

    /// Build the device-local `playback_state` row from the entry playing and
    /// where it stands, and save it — or clear it when playback has stopped.
    /// The queue itself is in its own tables; none of this syncs.
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
        let position_ms = match &self.slot {
            PlaybackSlot::Active(cur) => Some(cur.position.as_millis()),
            _ => None,
        };
        let row = DbPlaybackState {
            current: self.playback_queue.current().cloned(),
            position_ms,
            volume: self.volume.level(self.audio_output.as_ref()),
            is_muted: self.volume.is_muted(),
        };
        if let Err(e) = self.library_manager.save_playback_state(&row).await {
            warn!(
                "couldn't persist playback state (current entry {:?}): {e}",
                row.current
            );
            self.telemetry_anomaly(AnomalyKind::ResumePersistFailed);
        }
    }

    /// Set the output volume. While playing remotely, the device's volume is set
    /// too so it stays in step and survives the handoff back to local. Unmutes on
    /// a non-zero level, and always emits `VolumeChanged`.
    pub(super) fn set_volume(&mut self, volume: f32) {
        let lifted_mute = self.volume.set(volume, self.audio_output.as_ref());
        self.renderer.set_remote_volume(volume);
        if lifted_mute {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::MuteChanged { is_muted: false },
            );
        }
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::VolumeChanged { volume },
        );
    }

    pub(super) fn pause(&mut self) {
        // While playing remotely, the device holds playback: pause it too.
        self.renderer.pause_remote();
        // On AirPlay, decode is local: `sync_audio_state` below stops the sink
        // pulling, and a FLUSH drops the receiver's buffered audio so resume
        // starts clean rather than replaying the ~2 s already sent.
        // Pausing during a load collapses the Loading phase to Paused, so the
        // pending TrackReady no longer matches and is ignored. A Stopped or
        // still-resolving slot is a no-op (nothing is playing to pause).
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Paused(PausePhase::Manual);
            self.sync_audio_state();
            self.emit_state();
        }
        self.discard_buffered_listening();
        self.renderer.flush_airplay();
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

        // Pausing at a drained track flushed its receiver tail. Resume advances
        // that completed track instead of restarting an exhausted decoder.
        if let Some(completion) = self.audible_completion.take() {
            let track_id = completion.fmt.track_id.clone();
            self.publish_completion(completion.fmt, completion.errors, completion.samples);
            self.handle_auto_advance(track_id).await;
            return;
        }
        // Ask the system before anything plays. A refusal leaves the track
        // where it is and tells every UI, which may have shown it playing.
        if matches!(self.slot, PlaybackSlot::Active(_)) && !self.audio_focus_granted() {
            self.emit_state();
            return;
        }
        // Capture the listener before allowing the renderer to consume more audio.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Playing;
        }
        if !self.begin_listening().await {
            self.stop().await;
            return;
        }
        self.renderer.play_remote();
        // On AirPlay, re-anchor the pacing after the FLUSH so the resumed stream
        // starts fresh rather than bursting to catch a clock that ran during the
        // pause.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Playing;
            self.sync_audio_state();
            self.emit_state();
        }
        self.renderer.reanchor_airplay();
    }
}
