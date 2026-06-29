use super::*;

impl PlaybackService {
    /// Pause main player for preview. Called after all fallible setup so that
    /// error paths don't leave the main player paused with no preview.
    pub(super) fn pause_main_for_preview(&mut self) {
        if self.audio_output.get_state() == crate::playback::audio_output::AudioState::Playing {
            self.main_was_playing_before_preview = true;
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Paused);

            if self.current_prepared.is_some() && self.current_track_info.is_some() {
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::StateChanged {
                        state: self.make_paused_state(PlaybackPauseReason::Manual),
                    },
                );
            }
        }
    }

    /// Resume main player if it was paused for preview.
    pub(super) fn maybe_resume_main_player(&mut self) {
        if !self.main_was_playing_before_preview {
            return;
        }
        self.main_was_playing_before_preview = false;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Playing);

        if self.current_prepared.is_some() && self.current_track_info.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_playing_state(),
                },
            );
        }
    }

    /// Start decoding and streaming for preview playback.
    ///
    /// Shared by `handle_preview_play` (`seek_to=None`) and `handle_preview_seek`.
    /// Uses the buffer from `self.preview_buffer`. Returns true on success.
    pub(super) async fn start_preview_decode(
        &mut self,
        path: String,
        duration: std::time::Duration,
        sample_rate: u32,
        channels: u32,
        buffer: SharedSparseBuffer,
        seek_to: Option<std::time::Duration>,
        paused: bool,
        pause_main: bool,
    ) -> bool {
        // Preview has only Idle/Playing/Paused — no Loading/Buffering state to
        // confirm — so it emits Playing/Paused immediately and skips the
        // ready-watcher. The demand-driven local fill keeps the ring fed; the
        // audio callback outputs silence until the first samples land rather
        // than blocking on a fixed wait that could dead-end into a frozen state.
        let (mut sink, source, _) = create_track_stream_pair(sample_rate, channels);

        let decoder_buffer = buffer.clone();
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seek_to_sample = seek_to.map(|d| (d.as_secs_f64() * sample_rate as f64) as u64);
        let decoder_handle = std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(
                decoder_buffer,
                &mut sink,
                seek_to_sample,
                None,
                None,
                None, // preview auditions the whole file
                preview_cancel,
            ) {
                let _ = log_streaming_decode_failure("Preview decode", e);
            }
        });

        self.preview_decoder_handle = Some(decoder_handle);

        // Preview never chains; wrap in a PlaybackSource and drop the boundary
        // receiver so the sender's sends are no-ops. The fmt's values aren't
        // read by the preview listener (which captures `seek_offset` and the
        // duration locally), but they're set realistically for hygiene.
        let preview_fmt = TrackFmt {
            track_id: path.clone(),
            duration_ms: duration.as_millis() as u64,
            pregap_ms: None,
            position_offset: seek_to.unwrap_or(std::time::Duration::ZERO),
            // Preview plays an unimported file (no stored measurements) at unity.
            replay_gain_linear: 1.0,
        };
        let (preview_boundary_tx, _preview_boundary_rx) = tokio_mpsc::unbounded_channel();
        let source = Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            preview_fmt,
            preview_boundary_tx,
        )));

        if self.preview_audio_output.is_none() {
            match default_audio_output() {
                Ok(output) => self.preview_audio_output = Some(output),
                Err(e) => {
                    error!("Failed to create preview audio output: {:?}", e);
                    return false;
                }
            }
        }
        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }

        let setup = match setup_audio_stream(
            self.preview_audio_output.as_deref_mut().unwrap(),
            source.clone(),
            self.position_update_interval_ms,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create preview audio stream: {:?}", e);
                return false;
            }
        };

        if pause_main {
            self.pause_main_for_preview();
        }

        if let Err(e) = setup.stream.play() {
            error!("Failed to start preview playback: {:?}", e);
            for h in setup.bridge_handles {
                h.abort();
            }
            return false;
        }

        let preview_output = self.preview_audio_output.as_ref().unwrap();
        if paused {
            preview_output.set_state(crate::playback::audio_output::AudioState::Paused);
        } else {
            preview_output.set_state(crate::playback::audio_output::AudioState::Playing);
        }

        let seek_offset = seek_to.unwrap_or(std::time::Duration::ZERO);

        self.preview_stream = Some(setup.stream);
        self.preview_playback_source = Some(source.clone());
        self.preview_path = Some(path.clone());
        self.preview_position = seek_offset;
        self.preview_duration = duration;
        self.preview_seek_offset = seek_offset;

        let dur_ms = duration.as_millis() as u64;
        let preview_state = if paused {
            PreviewState::Paused {
                path: path.clone(),
                duration_ms: dur_ms,
            }
        } else {
            PreviewState::Playing {
                path: path.clone(),
                duration_ms: dur_ms,
            }
        };

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PreviewStateChanged(preview_state),
        );

        let progress_tx = self.progress_tx.clone();
        let command_tx = self.command_tx.clone();
        let mut position_rx_async = setup.position_rx;
        let mut completion_rx_async = setup.completion_rx;

        let h3 = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Preview is single-track; the audio callback tags every
                    // tick with the same fmt we built above. Read its fields
                    // directly so the listener doesn't carry parallel copies
                    // of seek_offset/dur_ms.
                    Some((fmt, pos)) = position_rx_async.recv() => {
                        let actual_pos_ms = (fmt.position_offset + pos).as_millis() as u64;
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::PreviewPositionUpdate {
                                position_ms: actual_pos_ms,
                                progress: crate::playback::format::compute_progress(actual_pos_ms, fmt.duration_ms, fmt.pregap_ms),
                            },
                        );
                    }
                    Some((_fmt, _error_count, _samples_decoded)) = completion_rx_async.recv() => {
                        // Preview doesn't track decode stats — it's a quick
                        // playback, not a library track. The stats carried by
                        // CompletionEvent (uniform with main playback) are
                        // dropped here by design.
                        dispatch_command(&command_tx, PlaybackCommand::PreviewCompleted);
                        break;
                    }
                    else => break,
                }
            }
        });

        self.preview_listener_handles = vec![setup.bridge_handles, vec![h3]]
            .into_iter()
            .flatten()
            .collect();

        true
    }

    /// Handle preview play: toggle same file off, switch files, or start new preview.
    pub(super) async fn handle_preview_play(&mut self, path: String) {
        // Same path: if playing or paused, dismiss (stop). If finished, replay.
        if self.preview_path.as_deref() == Some(&path) {
            let is_finished =
                self.preview_stream.is_none() && self.preview_playback_source.is_none();
            if is_finished {
                self.abort_preview_listeners();
                self.preview_path = None;
            } else {
                self.stop_preview();
                return;
            }
        }

        // If a different preview is active, stop it first (without resuming main)
        if self.preview_path.is_some() {
            self.stop_preview_without_resume();
        }

        let source_size = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                error!("Failed to stat preview file {}: {}", path, e);
                return;
            }
        };

        // Create sparse buffer and start local file reader
        let buffer = create_sparse_buffer(source_size);
        let read_config = AudioReadConfig {
            path: path.clone(),
            source_size,
        };
        let reader: Box<dyn AudioDataReader> = Box::new(LocalReader::new(read_config));
        reader.start_reading(buffer.clone(), self.progress_tx.clone());

        // Probe duration and sample rate from file
        let probe = {
            let probe_path = path.clone();
            tokio::task::spawn_blocking(move || {
                crate::audio_codec::probe_audio_from_path(&probe_path)
            })
            .await
            .unwrap_or(None)
        };
        let probed_duration = probe
            .as_ref()
            .map(|p| p.duration)
            .unwrap_or(std::time::Duration::ZERO);
        let sample_rate = probe.as_ref().map(|p| p.sample_rate).unwrap_or(44100);
        let channels = probe.as_ref().map(|p| p.channels).unwrap_or(2);

        self.preview_buffer = Some(buffer.clone());
        self.preview_sample_rate = sample_rate;
        self.preview_channels = channels;

        if self
            .start_preview_decode(
                path.clone(),
                probed_duration,
                sample_rate,
                channels,
                buffer,
                None,
                false,
                true,
            )
            .await
        {
            info!("Preview started: {}", path);
        }
    }

    /// Abort preview position/completion listener tasks.
    pub(super) fn abort_preview_listeners(&mut self) {
        for handle in self.preview_listener_handles.drain(..) {
            handle.abort();
        }
    }

    /// Stop preview playback and resume main player if it was paused for preview.
    pub(super) fn stop_preview(&mut self) {
        self.stop_preview_without_resume();
        self.maybe_resume_main_player();
    }

    /// Stop preview playback without resuming main player.
    pub(super) fn stop_preview_without_resume(&mut self) {
        if let Some(source) = self.preview_playback_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }

        // Cancel buffer to unblock decoder, then drop its handle
        if let Some(buf) = &self.preview_buffer {
            buf.cancel();
        }
        self.preview_buffer = None;
        self.preview_decoder_handle = None;

        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }
        if let Some(preview_output) = &self.preview_audio_output {
            preview_output.set_state(crate::playback::audio_output::AudioState::Stopped);
        }

        self.abort_preview_listeners();

        let was_previewing = self.preview_path.is_some();
        self.preview_path = None;
        self.preview_position = std::time::Duration::ZERO;
        self.preview_duration = std::time::Duration::ZERO;
        self.preview_seek_offset = std::time::Duration::ZERO;

        if was_previewing {
            info!("Preview stopped");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
        }
    }

    /// Handle preview toggle pause/resume.
    pub(super) async fn handle_preview_toggle_pause(&mut self) {
        let Some(path) = self.preview_path.clone() else {
            return;
        };

        let is_finished = self.preview_stream.is_none() && self.preview_playback_source.is_none();

        if is_finished {
            self.handle_preview_play(path).await;
            return;
        }

        let Some(preview_output) = &self.preview_audio_output else {
            return;
        };

        let dur_ms = self.preview_duration.as_millis() as u64;
        match preview_output.get_state() {
            crate::playback::audio_output::AudioState::Playing => {
                preview_output.set_state(crate::playback::audio_output::AudioState::Paused);

                // Record the current position so resume can continue from there.
                let position = self
                    .preview_playback_source
                    .as_ref()
                    .map(|s| self.preview_seek_offset + s.lock().unwrap().position())
                    .unwrap_or(self.preview_position);
                self.preview_position = position;

                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PreviewStateChanged(PreviewState::Paused {
                        path,
                        duration_ms: dur_ms,
                    }),
                );
            }
            crate::playback::audio_output::AudioState::Paused => {
                preview_output.set_state(crate::playback::audio_output::AudioState::Playing);

                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
                        path,
                        duration_ms: dur_ms,
                    }),
                );
            }
            _ => {}
        }
    }

    /// Handle natural preview completion: fully stop preview and resume main player.
    pub(super) fn handle_preview_completed(&mut self) {
        info!("Preview finished");
        self.stop_preview();
    }

    /// Seek within the active preview.
    pub(super) async fn handle_preview_seek(&mut self, position: std::time::Duration) {
        let buffer = match &self.preview_buffer {
            Some(buf) => buf.clone(),
            None => return,
        };
        if self.preview_path.is_none() {
            return;
        }
        let duration = self.preview_duration;
        if duration.is_zero() {
            return;
        }

        let was_paused = self
            .preview_audio_output
            .as_ref()
            .map(|o| o.get_state() == crate::playback::audio_output::AudioState::Paused)
            .unwrap_or(false);

        // Abort old listeners immediately to prevent stale position ticks
        self.abort_preview_listeners();
        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }

        // Tear down old decoder, preserve buffer
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        Self::teardown_decoder_for_seek(
            &mut self.preview_playback_source,
            &buffer,
            &preview_cancel,
            &mut self.preview_decoder_handle,
            false,
        )
        .await;

        // Start new decoder on the same buffer with seek_to
        let path = self.preview_path.clone().unwrap();
        let sample_rate = self.preview_sample_rate;
        let channels = self.preview_channels;
        self.start_preview_decode(
            path,
            duration,
            sample_rate,
            channels,
            buffer,
            Some(position),
            was_paused,
            false,
        )
        .await;

        // When seeking while paused, no tick will fire to carry the new
        // position — emit explicitly so the NSView updates. When seeking
        // while playing, the position listener task picks up from the new
        // offset on its next tick, so no explicit emit is needed.
        if was_paused {
            let pos_ms = position.as_millis() as u64;
            let dur_ms = duration.as_millis() as u64;
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewPositionUpdate {
                    position_ms: pos_ms,
                    progress: crate::playback::format::compute_progress(pos_ms, dur_ms, None),
                },
            );
        }
    }
}
