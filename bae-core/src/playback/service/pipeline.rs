use super::*;

impl PlaybackService {
    // Helper accessors for current/next track state
    pub(super) fn current_track_id(&self) -> Option<&str> {
        self.current_prepared
            .as_ref()
            .map(|p| p.track_info.track_id.as_str())
    }

    /// Abort current track listener tasks (position ticks + completion).
    pub(super) fn abort_current_listeners(&mut self) {
        if let Some(handle) = self.current_listener_handle.take() {
            handle.abort();
        }
    }

    /// Tear down the outgoing track before a manual switch (Play / SkipTo /
    /// Previous / Next-without-preload). Mirrors `stop()`'s current-track
    /// teardown — cancel the playback source so the callback goes silent, signal
    /// the decoder cancel token and cancel the buffer (so the decoder thread
    /// exits its park loop instead of filling a ring nobody pulls, and the
    /// data reader's fill loop stops fetching), and abort listeners — but leaves
    /// the audio state and the shared-buffer cache alone so the incoming track
    /// owns the transition. The buffer is spared when it's shared (the
    /// same-source reuse path appends into it via `uncancel`); a shared buffer
    /// is dropped wholesale by `stop()` instead.
    ///
    /// The decoder thread is not joined here: joining would block the command
    /// loop, and the cancel token plus buffer cancel already make it exit
    /// promptly. `play_track` overwrites `current_decoder_handle` with the new
    /// thread's handle right after.
    pub(super) fn teardown_current_track(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        if let Some(source) = self.current_playback_source.take() {
            match source.lock() {
                Ok(guard) => guard.cancel(),
                // A poisoned lock means the cpal callback thread panicked while
                // holding it. The source is being dropped here anyway, so the
                // cancel it would have requested is moot — log and move on.
                Err(_) => warn!("playback source lock poisoned during teardown; skipping cancel"),
            }
        }
        if let Some(prepared) = &self.current_prepared {
            prepared
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            if !prepared.buffer_shared {
                prepared.buffer.cancel();
            }
        }
        self.abort_current_listeners();
        self.current_prepared = None;
        self.current_decoder_handle = None;
    }

    /// Initialize streaming infrastructure without changing audio state.
    ///
    /// Sets up the cpal stream, position listeners, and completion handlers.
    /// The audio output state remains unchanged - caller must explicitly
    /// call `audio_output.set_state(Playing)` to start audio output.
    ///
    /// Returns true if initialization succeeded, false on error.
    pub(super) async fn init_streaming(&mut self, source: TrackStream, fmt: TrackFmt) -> bool {
        // Drop old stream first
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // fmt is the per-stream formatting envelope; the caller built it from
        // its prepared track (see `PlaybackPreparedTrack::track_fmt`). The
        // audio callback tags every position/completion emit with it; at a
        // gapless boundary the callback swaps to the staged next track's fmt.
        let position_offset = fmt.position_offset;

        // Wrap the track source in a PlaybackSource so the audio callback can
        // advance to a pre-staged next track without rebuilding the stream.
        let gapless = Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            fmt,
            self.boundary_tx.clone(),
        )));

        let setup = match setup_audio_stream(
            &mut *self.audio_output,
            gapless.clone(),
            self.position_update_interval_ms,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create streaming audio stream: {:?}", e);
                return false;
            }
        };

        if let Err(e) = setup.stream.play() {
            error!("Failed to start streaming playback: {:?}", e);
            return false;
        }

        // Update state
        self.stream = Some(setup.stream);
        self.current_playback_source = Some(gapless);
        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        // Spawn position/completion listener. Each event arrives tagged with
        // the fmt of the track it belongs to (set by the audio callback at
        // emit time), so the handlers are pure functions of their payload.
        let progress_tx = self.progress_tx.clone();
        let current_position_shared = self.current_position_shared.clone();
        let last_position_display = self.last_position_display.clone();
        let mut position_rx_async = setup.position_rx;
        let mut completion_rx_async = setup.completion_rx;

        let listener_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some((fmt, pos)) = position_rx_async.recv() => {
                        let actual_pos = fmt.position_offset + pos;
                        *current_position_shared.lock().unwrap() = Some(actual_pos);
                        let raw_pos_ms = actual_pos.as_millis() as u64;
                        let progress = crate::playback::format::compute_progress(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
                        *last_position_display.lock().unwrap() = Some(PositionDisplay {
                            progress,
                        });
                        let (adjusted_pos_ms, adjusted_dur_ms) =
                            crate::playback::format::adjust_for_pregap(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::PositionUpdate {
                                position_ms: adjusted_pos_ms,
                                duration_ms: adjusted_dur_ms,
                                track_id: fmt.track_id.clone(),
                                progress,
                            },
                        );
                    }
                    Some((fmt, error_count, samples_decoded)) = completion_rx_async.recv() => {
                        info!("Track completed: {} ({} decode errors, {} samples)", fmt.track_id, error_count, samples_decoded);
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::TrackCompleted {
                                track_id: fmt.track_id.clone(),
                            },
                        );
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::DecodeStats {
                                track_id: fmt.track_id.clone(),
                                error_count,
                                samples_decoded,
                            },
                        );
                        break;
                    }
                    else => break,
                }
            }
        });

        self.current_listener_handle = Some(listener_handle);

        true
    }

    /// Spawn the in-core decoder for the current prepared track, build the audio
    /// stream, and arm the ready-watcher — the shared tail of the play and seek
    /// paths. Reads the buffer and cancel token from `current_prepared` (the
    /// caller stages it first), so the watcher's `TrackReady` carries the live
    /// load's token and the handler ignores a stale signal.
    ///
    /// Returns whether `init_streaming` succeeded; the caller owns the
    /// failure path (its cleanup differs) and any audio-state change.
    /// Audio doesn't flow until the ring fills, so the caller may set the
    /// audio state after this returns without racing the watcher.
    pub(super) async fn start_decoder_and_watch(
        &mut self,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        sample_rate: u32,
        channels: u32,
        track_id: String,
    ) -> bool {
        let prepared = self
            .current_prepared
            .as_ref()
            .expect("start_decoder_and_watch requires a staged current_prepared");
        let decoder_buffer = prepared.buffer.clone();
        let cancel_token = prepared.cancel_token.clone();

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
        self.current_decoder_handle = Some(decoder_handle);

        if !self.init_streaming(source, fmt).await {
            return false;
        }

        // Hold Playing until audio is actually flowing. The in-core decoder
        // signals `ready_rx` when the ring buffer fills to the play threshold
        // (or hits EOF for a short track); a watcher task turns that into a
        // `TrackReady` command so the command loop stays responsive to
        // Stop/Pause during a slow cloud load. Awaiting inline would wedge the
        // loop.
        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            // Err means the decoder dropped its sink before signalling ready (it
            // died or was cancelled). The decode-failure path drives playback to
            // Stopped, and a cancelled load is no longer current so TrackReady
            // would be ignored anyway — the error path owns recovery, so just
            // record the dropped watcher.
            match ready_rx.await {
                Ok(()) => dispatch_command(
                    &command_tx,
                    PlaybackCommand::TrackReady {
                        track_id,
                        cancel_token,
                    },
                ),
                Err(_) => {
                    debug!("ready watcher dropped before signal for track {track_id}")
                }
            }
        });

        true
    }

    /// Play a track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    pub(super) async fn play_track(
        &mut self,
        track_id: &str,
        is_natural_transition: bool,
        preserve_paused: bool,
    ) {
        info!(
            "Playing track: {} (natural_transition: {}, preserve_paused: {})",
            track_id, is_natural_transition, preserve_paused
        );

        // Tear down the outgoing track up front so a manual switch silences the
        // old audio immediately and frees the old decoder + reader, instead of
        // leaving them running until this method overwrites the current state at
        // the end. Spares a shared source buffer the incoming track reuses.
        self.teardown_current_track();

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.to_string(),
                    resolved: None,
                },
            },
        );

        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = prepare_track_for_playback(
            &self.library_manager,
            track_id,
            &mut self.shared_file_buffer,
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

        // Metadata is resolved now: re-emit Loading carrying the target track's
        // info so the bar switches from the prior track to the target while the
        // first audio bytes are still downloading.
        let loading_duration_ms = pregap_adjusted_duration(&prepared);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.to_string(),
                    resolved: Some(LoadingTrack {
                        track_info: prepared.track_info.clone(),
                        duration_ms: loading_duration_ms,
                    }),
                },
            },
        );

        // Calculate pregap seek position if needed (direct selection skips pregap)
        let pregap_skip_duration = pregap_seek_position(prepared.pregap_ms, is_natural_transition);

        // Seek to the track's first sample plus any pregap skip; trim lead-in
        // there and stop at the track's end.
        let pregap_offset = match pregap_skip_duration {
            Some(d) => (d.as_secs_f64() * prepared.sample_rate as f64) as u64,
            None => 0,
        };
        let decode = prepared.decode_params(pregap_offset);

        // Position offset: when we skip pregap, decoder positions start at 0 but actual
        // track position is pregap_ms
        let position_offset = if pregap_skip_duration.is_some() {
            std::time::Duration::from_millis(prepared.pregap_ms.unwrap_or(0).max(0) as u64)
        } else {
            std::time::Duration::ZERO
        };
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;
        let fmt = prepared.track_fmt(position_offset);

        // Store prepared track state so the shared tail reads this load's buffer
        // and cancel token.
        self.current_prepared = Some(prepared);
        // This track is now the one the user is waiting to hear: its reader
        // fetches with priority, the next-track preload below yields to it.
        self.mark_current_foreground();
        if !self
            .start_decoder_and_watch(decode, fmt, sample_rate, channels, track_id.to_string())
            .await
        {
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

        // Set audio state: always Playing unless preserving paused state. Audio
        // doesn't flow until the ring fills, so this doesn't race the watcher.
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Playing);
        }

        info!("Streaming playback started for track: {}", track_id);

        self.preload_queue_front().await;

        // Persist the now-playing state so a restart on this device resumes here.
        self.persist_playback_state().await;
    }

    pub(super) async fn stop(&mut self) {
        self.pending_side_pause = None;
        // Stop any active preview first (without resuming main, since we're stopping)
        self.preview.stop();
        self.main_was_playing_before_preview = false;

        // Tear down the current track (stream, source, buffer, decoder,
        // listeners) — the half `stop()` shares with a manual track switch.
        self.teardown_current_track();

        // Stop-specific teardown beyond the current track:
        self.clear_next_track_state();
        // Drop shared buffer cache — stop means we're done with this album
        self.shared_file_buffer = None;
        *self.current_position_shared.lock().unwrap() = None;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Stopped);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Stopped,
            },
        );
        // Audio is now Stopped, so this clears the durable row — covering every
        // stop path (natural end, halt-on-error, the current track removed),
        // not just the explicit Stop command.
        self.persist_playback_state().await;
    }
}
