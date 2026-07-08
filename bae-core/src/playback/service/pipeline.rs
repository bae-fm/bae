use super::*;

impl PlaybackService {
    /// Tear down the current track, whatever phase it is in, leaving the slot
    /// Stopped. The pipeline's `cancel()` silences the audio callback, signals the
    /// decoder token, and drops the stream; cancelling the prepared track's
    /// unshared buffers stops the data reader and lets the decoder thread exit its
    /// park loop.
    ///
    /// A shared buffer is spared (the same-source reuse path appends into it via
    /// `uncancel`); `stop()` drops the shared-buffer cache wholesale instead. The
    /// decoder thread is not joined — joining would block the command loop, and
    /// the cancel token plus buffer cancel already make it exit promptly; a fresh
    /// load's decoder replaces it.
    pub(super) fn teardown_current_track(&mut self) {
        if let PlaybackSlot::Active(cur) = std::mem::replace(&mut self.slot, PlaybackSlot::Stopped)
        {
            cur.pipeline.cancel();
            cur.prepared.cancel_unshared_buffers();
            // Dropping `cur` drops the audio-events receiver.
        }
    }

    /// Install a freshly built stream as the current track, in `phase`, and
    /// project its intent onto the audio-state atomic. Also hands this track's
    /// reader fetch priority. Does not emit a `StateChanged` — the caller decides
    /// whether the transition surfaces one now (skip, gapless advance) or waits
    /// for the ready-watcher's `TrackReady` (fresh play, seek).
    pub(super) fn install_active_track(
        &mut self,
        prepared: PlaybackPreparedTrack,
        pipeline: StreamPipeline,
        audio_events: AudioEventReceiver,
        phase: TrackPhase,
    ) {
        self.slot = PlaybackSlot::Active(CurrentTrack {
            prepared,
            pipeline,
            audio_events,
            phase,
        });
        self.mark_current_foreground();
        self.sync_audio_state();
    }

    /// Spawn the in-core decoder for `prepared`, build the audio stream through
    /// the shared `StreamPipeline`, and arm the ready-watcher — the shared tail of
    /// the play and seek paths. The watcher's `TrackReady` carries this load's
    /// `generation` so the handler ignores a stale signal. Audio doesn't flow
    /// until the ring fills, so the caller may set the phase target after this
    /// returns without racing the watcher.
    ///
    /// On stream-build failure `start_stream_pipeline` has already cancelled the
    /// just-spawned decoder; here we cancel the prepared track's unshared buffers
    /// and wake their readers (so a read-blocked decoder sees the token), then
    /// return the error for the caller to resolve.
    pub(super) async fn start_decoder_and_watch(
        &mut self,
        prepared: &PlaybackPreparedTrack,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        generation: LoadGeneration,
    ) -> Result<(StreamPipeline, AudioEventReceiver), PlaybackError> {
        let position_offset = fmt.position_offset;
        let track_id = prepared.track_info.track_id.clone();

        let start = match start_stream_pipeline(
            &mut *self.audio_output,
            decode,
            fmt,
            prepared.sample_rate,
            prepared.channels,
            self.position_update_interval_ms,
            "Streaming decode",
            DecodeFailureReport::EmitPlaybackError {
                progress_tx: self.progress_tx.clone(),
            },
        )
        .await
        {
            Ok(start) => start,
            Err(e) => {
                prepared.cancel_unshared_buffers();
                for segment in &prepared.segments {
                    segment.buffer.wake_readers();
                }
                return Err(e);
            }
        };

        // The stream's start offset is the in-track time it begins at (non-zero
        // only on seek). Set here rather than inside the shared unit — preview has
        // no shared position cell.
        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        // Hold the phase in Loading until audio is actually flowing. The in-core
        // decoder signals `ready` when the ring buffer fills to the play threshold
        // (or hits EOF for a short track); a watcher task turns that into a
        // `TrackReady` command so the command loop stays responsive to Stop/Pause
        // during a slow cloud load. Awaiting inline would wedge the loop.
        let command_tx = self.command_tx.clone();
        let ready = start.ready;
        tokio::spawn(async move {
            // Err means the decoder dropped its sink before signalling ready (it
            // died or was cancelled). The decode-failure path drives playback to
            // Stopped, and a cancelled load's generation no longer matches so
            // TrackReady would be ignored anyway — the error path owns recovery,
            // so just record the dropped watcher.
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

        Ok((start.pipeline, start.audio_events))
    }

    /// Play a track.
    /// - `start`: selects direct starts, natural transitions, or a restored raw position
    /// - `target`: where the load lands once audio is ready (Playing, or Paused
    ///   with a reason). Computed absolutely by the caller.
    pub(super) async fn play_track(
        &mut self,
        track_id: &str,
        start: TrackStart,
        target: PlayTarget,
    ) {
        info!(
            "Playing track: {} (start: {:?}, target: {:?})",
            track_id, start, target
        );

        // Tear down the outgoing track up front so a manual switch silences the
        // old audio immediately and frees the old decoder + reader, instead of
        // leaving them running until this method assembles the new track at the
        // end. Spares a shared source buffer the incoming track reuses.
        self.teardown_current_track();
        self.clear_next_track_state();

        // First Loading emission: bare, before the metadata lookup.
        self.slot = PlaybackSlot::Loading {
            track_id: track_id.to_string(),
            resolved: None,
        };
        self.sync_audio_state();
        self.emit_state();

        // Prepare track: fetch metadata, create buffer, start reading
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
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: e.into_ui_reason(),
                    },
                );
                self.stop().await;
                return;
            }
        };

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

        let (pipeline, audio_events) = match self
            .start_decoder_and_watch(&prepared, decode, fmt, generation)
            .await
        {
            Ok(parts) => parts,
            Err(_) => {
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

        // Assemble the current track. The phase stays Loading (with this load's
        // generation and target) until the ready-watcher's TrackReady resolves it
        // — no StateChanged is emitted here; the second Loading above is the last
        // emission until audio flows. `install_active_track` projects the target
        // onto the atomic so the callback outputs samples/silence as intended.
        self.install_active_track(
            prepared,
            pipeline,
            audio_events,
            TrackPhase::Loading { generation, target },
        );

        info!("Streaming playback started for track: {}", track_id);

        self.preload_queue_front().await;

        // Persist the now-playing state so a restart on this device resumes here.
        self.persist_playback_state().await;
    }

    pub(super) async fn stop(&mut self) {
        // Stop any active preview first (without resuming main, since we're stopping)
        self.stop_preview_for_main_playback();

        // Tear down the current track (stream, source, buffer, decoder,
        // listeners) — the half `stop()` shares with a manual track switch.
        self.teardown_current_track();

        // Stop-specific teardown beyond the current track:
        self.clear_next_track_state();
        // Drop shared buffer cache — stop means we're done with this album
        self.shared_file_buffers.clear();
        *self.current_position_shared.lock().unwrap() = None;
        // The slot is Stopped; project that onto the atomic and emit Stopped.
        self.sync_audio_state();
        self.emit_state();
        // Audio is now Stopped, so this clears the durable row — covering every
        // stop path (natural end, halt-on-error, the current track removed),
        // not just the explicit Stop command.
        self.persist_playback_state().await;
    }
}
