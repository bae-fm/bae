use super::*;

impl PlaybackService {
    /// Install the incoming track as current, in `phase`, and project its intent
    /// onto the audio-state atomic. Also hands this track's reader fetch priority.
    /// Does not emit a `StateChanged` — the caller decides whether the transition
    /// surfaces one now (skip, gapless advance) or waits for the ready-watcher's
    /// `TrackReady` (fresh play, seek). The stream/source/audio-events receiver
    /// are already in `self.output`.
    pub(super) fn install_active_track(
        &mut self,
        prepared: PlaybackPreparedTrack,
        decoder: TrackDecoder,
        phase: TrackPhase,
    ) {
        self.slot = PlaybackSlot::Active(CurrentTrack {
            prepared,
            decoder,
            phase,
        });
        self.mark_current_foreground();
        self.sync_audio_state();
        // A fresh install starts this track's starvation clock over, whatever
        // the outgoing track's watchdog state was.
        self.reset_starvation_episode();
    }

    /// Play `track_id` from scratch: tear down whatever was playing, resolve and
    /// prepare it, spawn its decoder, and install it as current.
    /// - `start`: a direct start (pregap skipped), a natural transition (pregap
    ///   played), or a restored raw position.
    /// - `target`: where the load lands once audio is ready (Playing, or Paused
    ///   with a reason). Computed absolutely by the caller.
    /// - `transition`: how this start came about, for the `TrackStarted` event.
    ///   Reported here at initiation for every caller that reaches `play_track`
    ///   with a `Playing` target; a `Paused` target loads without playing and
    ///   ships nothing.
    pub(super) async fn play_track(
        &mut self,
        track_id: &str,
        start: TrackStart,
        target: PlayTarget,
        transition: TrackTransition,
    ) {
        // Stamp the first-audio clock at the very start of the load, before any
        // teardown/prepare work, so the measured wait covers the whole path to
        // Playing.
        let play_started_at = std::time::Instant::now();
        info!(
            "Playing track: {} (start: {:?}, target: {:?})",
            track_id, start, target
        );

        // Report the start now — before prepare/decoder, which may fail and ship
        // their own `playback_failed`. A paused-target load isn't a start.
        if matches!(target, PlayTarget::Playing) {
            self.telemetry_track_started(track_id, transition);
        }

        // A remote renderer is a second renderer behind the same queue: load the
        // track onto the device instead of the local decode pipeline. Everything
        // above (telemetry) is shared; the local path below is skipped.
        if self.renderer.is_remote() {
            self.play_track_remote(track_id, start, target).await;
            return;
        }

        // Tear the outgoing track and preload down first, so a manual switch
        // silences the old audio at once and stops the old decoders. Their file
        // buffers stay cached and live until the incoming track is prepared — only
        // then is it known which files it shares (a CUE album's tracks all play
        // from one file, whose buffer and fill task must survive the switch).
        self.retire_current_track();
        self.retire_preloaded_track();

        // First Loading emission: bare, before the metadata lookup.
        self.slot = PlaybackSlot::Loading {
            track_id: track_id.to_string(),
            resolved: None,
        };
        self.sync_audio_state();
        self.emit_state();

        let prepared = prepare_track_for_playback(
            &self.library_manager,
            track_id,
            &mut self.file_buffers,
            &self.command_tx,
        )
        .await;
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to prepare track {}: {}", track_id, e);
                self.telemetry_playback_failed(crate::diagnostics::PlaybackOperation::LoadContext);
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: e.into_ui_reason(),
                    },
                );
                // stop()'s cache-wide cancel releases the outgoing tracks'
                // buffers.
                self.stop().await;
                return;
            }
        };

        // The incoming track's files are known now: release the outgoing current
        // and preload buffers, keeping any file the new track also plays.
        self.file_buffers.release_retired(&prepared.file_ids());

        // Second Loading emission: carries the target track's metadata so the bar
        // switches from the prior track to the target while audio still downloads.
        self.slot = PlaybackSlot::Loading {
            track_id: track_id.to_string(),
            resolved: Some(LoadingTrack::from_prepared(&prepared)),
        };
        self.emit_state();

        let start_position = start.position(prepared.total_pregap_ms());
        let include_pregap = start.includes_pregap();
        let start_sample_offset = if include_pregap {
            (start_position.as_secs_f64() * prepared.sample_rate as f64) as u64
        } else {
            0
        };
        let decode = prepared.decode_params(start_sample_offset, include_pregap);
        let fmt = prepared.track_fmt(start_position);
        let generation = self.next_load_generation();

        // Measure time-to-Playing only for a load that actually plays; a paused
        // target loads without playing, and clears any prior pending measurement
        // so a superseded one never resolves.
        self.first_audio_pending =
            matches!(target, PlayTarget::Playing).then(|| FirstAudioMeasurement {
                generation,
                track_id: track_id.to_string(),
                started_at: play_started_at,
            });

        if self
            .start_decoder_and_install(
                prepared,
                decode,
                fmt,
                generation,
                StagedNextOnReplace::Discard,
                TrackPhase::Loading { generation, target },
                None,
            )
            .await
            .is_err()
        {
            self.telemetry_playback_failed(crate::diagnostics::PlaybackOperation::AudioOutputInit);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Couldn't start audio output for the track.",
                    ),
                },
            );
            self.stop().await;
            return;
        }

        info!("Streaming playback started for track: {}", track_id);

        self.preload_queue_front().await;

        // Persist the now-playing state so a restart on this device resumes here.
        self.persist_playback_state().await;
    }

    pub(super) async fn stop(&mut self) {
        // Stop means stop, on whichever renderer: while playing remotely, end the
        // device session (pause is what keeps it warm) and return to local before
        // the teardown lands the slot in Stopped. A caller that already dropped to
        // local (`fail_remote`, `end_remote_and_resume_local`) skips this.
        let was_playing_on_device = self
            .renderer
            .return_to_local_for_stop(&mut self.audio_output);
        if was_playing_on_device {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::RemoteStatusChanged { device_name: None },
            );
        }
        // Tear the local pipeline down: preview, current decoder, preload, the
        // persistent output (its source cancelled first so a ring-parked decoder
        // unparks), and every cached file buffer. Shared with the remote switch,
        // which reuses the same teardown before installing the remote track.
        self.teardown_local_playback();
        self.sync_audio_state();
        self.emit_state();
        // With the slot Stopped this clears the durable row — on every stop path
        // (natural end, halt-on-error, the current track removed), not just the
        // explicit Stop command.
        self.persist_playback_state().await;
    }
}
