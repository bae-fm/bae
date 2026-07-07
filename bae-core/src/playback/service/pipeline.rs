use super::*;

/// A built, playing cpal (or test) stream over a decoded `TrackStream`: the
/// `StreamSetup` (stream + its audio-events receiver) plus the wrapped source.
/// `start_decoder_and_watch` pairs this with the decoder handle it spawned; the
/// preload-advance paths pair it with the already-running preloaded decoder.
pub(super) struct StreamParts {
    pub(super) setup: StreamSetup,
    pub(super) source: Arc<Mutex<source::PlaybackSource>>,
}

/// Everything a fresh decoder load assembles: the stream parts plus the decoder
/// thread the caller stores in the `CurrentTrack`. Returned by
/// `start_decoder_and_watch` so the caller assigns the slot as one whole value.
pub(super) struct StartedStream {
    pub(super) parts: StreamParts,
    pub(super) decoder_handle: std::thread::JoinHandle<()>,
}

impl PlaybackService {
    /// Tear down the current track, whatever phase it is in, leaving the slot
    /// Stopped. Cancels the playback source so the callback goes silent, signals
    /// the decoder cancel token and cancels its unshared buffers (so the decoder
    /// thread exits its park loop and the data reader stops fetching), and drops
    /// the stream, audio-events receiver, and (detaching) the decoder handle.
    ///
    /// A shared buffer is spared (the same-source reuse path appends into it via
    /// `uncancel`); `stop()` drops the shared-buffer cache wholesale instead. The
    /// decoder thread is not joined — joining would block the command loop, and
    /// the cancel token plus buffer cancel already make it exit promptly; a fresh
    /// load's decoder replaces it.
    pub(super) fn teardown_current_track(&mut self) {
        if let PlaybackSlot::Active(cur) = std::mem::replace(&mut self.slot, PlaybackSlot::Stopped)
        {
            match cur.source.lock() {
                Ok(guard) => guard.cancel(),
                // A poisoned lock means the cpal callback thread panicked while
                // holding it. The source is being dropped here anyway, so the
                // cancel it would have requested is moot — log and move on.
                Err(_) => warn!("playback source lock poisoned during teardown; skipping cancel"),
            }
            cur.prepared
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            cur.prepared.cancel_unshared_buffers();
            // Dropping `cur` drops the stream and audio-events receiver and
            // detaches the decoder thread.
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
        started: StartedStream,
        phase: TrackPhase,
    ) {
        self.slot = PlaybackSlot::Active(CurrentTrack {
            prepared,
            stream: started.parts.setup.stream,
            source: started.parts.source,
            decoder_handle: started.decoder_handle,
            audio_events: started.parts.setup.audio_events,
            phase,
        });
        self.mark_current_foreground();
        self.sync_audio_state();
    }

    /// Build the cpal (or test) stream over a decoded `TrackStream`, start it,
    /// and return the parts. Sets the shared position to the stream's start
    /// offset. Does not touch the slot or the audio-state atomic — the caller
    /// assembles the slot and calls `sync_audio_state`.
    pub(super) async fn init_streaming(
        &mut self,
        source: TrackStream,
        fmt: TrackFmt,
    ) -> Result<StreamParts, PlaybackError> {
        // fmt is the per-stream formatting envelope; the caller built it from its
        // prepared track (see `PlaybackPreparedTrack::track_fmt`). The audio
        // callback tags every position/completion emit with it; at a gapless
        // boundary the callback swaps to the staged next track's fmt.
        let position_offset = fmt.position_offset;

        // Wrap the track source in a PlaybackSource so the audio callback can
        // advance to a pre-staged next track without rebuilding the stream.
        let gapless = Arc::new(Mutex::new(source::PlaybackSource::new(source, fmt)));

        let setup = setup_audio_stream(
            &mut *self.audio_output,
            gapless.clone(),
            self.position_update_interval_ms,
        )
        .await?;

        if let Err(e) = setup.stream.play() {
            return Err(PlaybackError::task(format!(
                "Failed to start streaming playback: {e:?}"
            )));
        }

        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        Ok(StreamParts {
            setup,
            source: gapless,
        })
    }

    /// Spawn the in-core decoder for `prepared`, build the audio stream, and arm
    /// the ready-watcher — the shared tail of the play and seek paths. The
    /// decoder reads `prepared`'s buffer and cancel token, so the watcher's
    /// `TrackReady` carries this load's `generation` and the handler ignores a
    /// stale signal. Audio doesn't flow until the ring fills, so the caller may
    /// set the phase target after this returns without racing the watcher.
    ///
    /// On stream-build failure the just-spawned decoder is cancelled (it would
    /// otherwise fill a ring nobody pulls) and the error is returned; the caller
    /// owns the rest of the failure path.
    pub(super) async fn start_decoder_and_watch(
        &mut self,
        prepared: &PlaybackPreparedTrack,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        generation: LoadGeneration,
    ) -> Result<StartedStream, PlaybackError> {
        let decoder_buffer = prepared
            .segments
            .first()
            .expect("prepared track has at least one segment")
            .buffer
            .clone();
        let cancel_token = prepared.cancel_token.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;
        let track_id = prepared.track_info.track_id.clone();

        // Create decoder sink/source with the track's sample rate and spawn the
        // in-core FFmpeg decoder thread that fills the sink's ring buffer.
        let (mut sink, source, ready_rx) = create_track_stream_pair(sample_rate, channels);

        let decoder_handle = {
            let decoder_cancel = cancel_token.clone();
            // Kept to tell a genuine decode failure from normal teardown (seek /
            // stop / track change all cancel the token before the decoder exits).
            let teardown_check = cancel_token.clone();
            let error_tx = self.progress_tx.clone();
            std::thread::spawn(move || {
                if let Err(e) = decode.run_decoder(decoder_buffer, &mut sink, decoder_cancel) {
                    if let Some(message) = log_streaming_decode_failure("Streaming decode", e) {
                        if !teardown_check.load(std::sync::atomic::Ordering::Relaxed) {
                            emit_progress(
                                &error_tx,
                                PlaybackProgress::PlaybackError {
                                    reason: crate::ui::PlaybackErrorReason::internal(format!(
                                        "Playback decode failed: {message}"
                                    )),
                                },
                            );
                        }
                    }
                }
            })
        };

        let parts = match self.init_streaming(source, fmt).await {
            Ok(parts) => parts,
            Err(e) => {
                error!("Failed to create streaming audio stream: {:?}", e);
                // The decoder is spawned but no stream will pull from it. Cancel
                // it so it exits instead of parking on a ring nobody reads, then
                // detach — joining would block the command loop.
                cancel_token.store(true, std::sync::atomic::Ordering::Release);
                prepared.cancel_unshared_buffers();
                for segment in &prepared.segments {
                    segment.buffer.wake_readers();
                }
                drop(decoder_handle);
                return Err(e);
            }
        };

        // Hold the phase in Loading until audio is actually flowing. The in-core
        // decoder signals `ready_rx` when the ring buffer fills to the play
        // threshold (or hits EOF for a short track); a watcher task turns that
        // into a `TrackReady` command so the command loop stays responsive to
        // Stop/Pause during a slow cloud load. Awaiting inline would wedge the loop.
        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            // Err means the decoder dropped its sink before signalling ready (it
            // died or was cancelled). The decode-failure path drives playback to
            // Stopped, and a cancelled load's generation no longer matches so
            // TrackReady would be ignored anyway — the error path owns recovery,
            // so just record the dropped watcher.
            match ready_rx.await {
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

        Ok(StartedStream {
            parts,
            decoder_handle,
        })
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

        let started = match self
            .start_decoder_and_watch(&prepared, decode, fmt, generation)
            .await
        {
            Ok(started) => started,
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
            started,
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
