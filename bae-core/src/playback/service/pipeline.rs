use super::*;

impl PlaybackService {
    /// Tear down the current track, whatever phase it is in, leaving the slot
    /// Stopped. Cancels the outgoing decoder's AVIO token (stopping its reads) and
    /// detaches its thread handle without joining — joining would block the
    /// command loop; a fresh load's decoder replaces it. Does NOT touch
    /// `self.output`: the outgoing decoder's ring (sink side) is cancelled by
    /// whichever teardown site follows — `PlaybackSource::replace` on a same-format
    /// switch, `build_output_over`'s source-cancel on a format-change rebuild, or
    /// `stop()`'s source-cancel before it drops the output. The AVIO token alone
    /// doesn't unpark a decoder blocked writing a full ring; that sink-side cancel
    /// does, which is why every site does it.
    ///
    /// Returns the outgoing prepared track: its file buffers are still cached
    /// and live, and the caller releases them (`release_buffers`) once it knows
    /// which files the incoming track keeps — or lets `stop()`'s cache-wide
    /// cancel cover them.
    pub(super) fn teardown_current_track(&mut self) -> Option<PlaybackPreparedTrack> {
        if let PlaybackSlot::Active(cur) = std::mem::replace(&mut self.slot, PlaybackSlot::Stopped)
        {
            cur.decoder
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            // Dropping `cur.decoder.handle` detaches the thread without joining.
            Some(cur.prepared)
        } else {
            None
        }
    }

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

    /// Spawn the in-core decoder for `prepared`, attach it to the persistent
    /// output (replace-in-place if the format matches, rebuild otherwise), and arm
    /// the ready-watcher — the shared tail of the play and seek paths. The
    /// watcher's `TrackReady` carries this load's `generation` so the handler
    /// ignores a stale signal. Audio doesn't flow until the ring fills, so the
    /// caller may set the phase target after this returns without racing the
    /// watcher.
    ///
    /// On attach failure (a format-change rebuild whose stream build failed) the
    /// just-spawned decoder is cancelled (its token set, the prepared track's
    /// buffer readers woken so a read-blocked decoder sees it) and its handle
    /// dropped, then the error is returned for the caller to resolve (both callers
    /// `stop()`, whose cache-wide cancel then releases the buffers themselves).
    pub(super) async fn start_decoder_and_watch(
        &mut self,
        prepared: &PlaybackPreparedTrack,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        generation: LoadGeneration,
    ) -> Result<TrackDecoder, PlaybackError> {
        let position_offset = fmt.position_offset;
        let track_id = prepared.track_info.track_id.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;

        let DecoderStart {
            track_stream,
            handle,
            cancel_token,
            ready,
        } = spawn_decoder(
            decode,
            sample_rate,
            channels,
            "Streaming decode",
            DecodeFailureReport::EmitPlaybackError {
                progress_tx: self.progress_tx.clone(),
            },
        );

        if let Err(e) = self
            .attach_track(track_stream, fmt, sample_rate, channels)
            .await
        {
            // No output pulls this decoder's ring. Cancel its token and wake the
            // byte buffers so a read-blocked decoder sees it, then detach.
            cancel_token.store(true, std::sync::atomic::Ordering::Release);
            for segment in &prepared.segments {
                segment.buffer.wake_readers();
            }
            drop(handle);
            return Err(e);
        }

        // The stream's start offset is the in-track time it begins at (non-zero
        // only on seek).
        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        // The decoder signals `ready` once its ring fills to the play threshold (or
        // hits EOF on a short track). A watcher task turns that into a `TrackReady`
        // command, keeping the command loop responsive to Stop/Pause during a slow
        // cloud load — awaiting it inline would wedge the loop.
        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            // Err means the decoder dropped its sink before signalling ready (it
            // died or was cancelled). The decode-failure path drives playback to
            // Stopped, and a cancelled load's generation no longer matches, so a
            // TrackReady would be ignored anyway — just record the dropped watcher.
            match ready.await {
                Ok(()) => dispatch_command(
                    &command_tx,
                    PlaybackCommand::TrackReady {
                        track_id,
                        generation,
                    },
                ),
                Err(_) => {
                    debug!("ready watcher dropped before signal for track {track_id}")
                }
            }
        });

        Ok(TrackDecoder {
            handle,
            cancel_token,
        })
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
        let outgoing = self.teardown_current_track();
        let outgoing_preload = self.take_preloaded_prepared();

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
            &mut self.shared_file_buffers,
            self.progress_tx.clone(),
            self.fetch_arbiter.clone(),
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
        let retained_file_ids = prepared.file_ids();
        for old in [outgoing, outgoing_preload].into_iter().flatten() {
            old.release_buffers(&retained_file_ids, &mut self.shared_file_buffers);
        }

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

        let decoder = match self
            .start_decoder_and_watch(&prepared, decode, fmt, generation)
            .await
        {
            Ok(decoder) => decoder,
            Err(_) => {
                self.telemetry_playback_failed(
                    crate::diagnostics::PlaybackOperation::AudioOutputInit,
                );
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
        };

        // The phase stays Loading (with this load's generation and target) until
        // the ready-watcher's TrackReady resolves it, so no StateChanged is
        // emitted here — the second Loading above is the last emission until audio
        // flows. `install_active_track` projects the target onto the atomic so the
        // callback outputs samples or silence as intended.
        self.install_active_track(
            prepared,
            decoder,
            TrackPhase::Loading { generation, target },
        );

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
        if let Renderer::Remote(remote) = &self.renderer {
            remote.session.stop();
        }
        // On AirPlay, decode is local: take the saved local sink out now so it can
        // be restored after the teardown below drops the AirPlay output (which
        // tears the receiver session down). Both remote flavors return to local.
        let was_playing_on_device = !matches!(self.renderer, Renderer::Local);
        let restore_local_output = match std::mem::replace(&mut self.renderer, Renderer::Local) {
            Renderer::AirPlay(airplay) => Some(airplay.saved_output),
            _ => None,
        };
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
        // Restore the local output sink after the AirPlay one (and its session)
        // is gone, so the next play uses the DAC again.
        if let Some(saved_output) = restore_local_output {
            self.audio_output = saved_output;
        }
        self.sync_audio_state();
        self.emit_state();
        // With the slot Stopped this clears the durable row — on every stop path
        // (natural end, halt-on-error, the current track removed), not just the
        // explicit Stop command.
        self.persist_playback_state().await;
    }
}
