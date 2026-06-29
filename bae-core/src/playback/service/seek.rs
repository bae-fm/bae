use super::*;

impl PlaybackService {
    /// Tear down the decoded-audio pipeline for a seek.
    ///
    /// Cancels reads to unblock the old decoder, but the data reader keeps
    /// filling the buffer. After this, the buffer is ready for a new decoder
    /// to read from position 0.
    pub(super) async fn teardown_decoder_for_seek(
        source: &mut Option<Arc<Mutex<source::PlaybackSource>>>,
        buffer: &SharedSparseBuffer,
        cancel_token: &Arc<std::sync::atomic::AtomicBool>,
        decoder_handle: &mut Option<std::thread::JoinHandle<()>>,
        buffer_shared: bool,
    ) {
        // Cancel streaming source (cpal callback outputs silence)
        if let Some(src) = source.take() {
            if let Ok(guard) = src.lock() {
                guard.cancel();
            }
        }

        // Cancel this decoder's AVIO reads via its token.
        cancel_token.store(true, std::sync::atomic::Ordering::Release);

        // For non-shared buffers, also cancel buffer reads to unblock the reader.
        // For shared buffers, only the cancel_token is used — other decoders
        // (e.g. preloaded next track) must not be disturbed.
        if !buffer_shared {
            buffer.cancel_reads();
        }
        // Wake up any readers blocked on the condvar so they can check the cancel token
        buffer.wake_readers();

        // Wait for decoder thread to exit. Surface a thread panic as an error
        // (decoder bug, real signal); tokio join failures (panic in the
        // spawn_blocking wrapper itself, runtime shutdown) get a warn.
        if let Some(handle) = decoder_handle.take() {
            match tokio::task::spawn_blocking(move || handle.join()).await {
                Ok(Ok(())) => {}
                Ok(Err(panic)) => {
                    error!("Decoder thread panicked during seek teardown: {:?}", panic);
                }
                Err(e) => {
                    warn!("spawn_blocking failed while joining decoder thread: {e}");
                }
            }
        }

        // Uncancel buffer reads for new decoders (only needed for non-shared)
        if !buffer_shared {
            buffer.uncancel();
        }
    }

    pub(super) async fn seek(&mut self, position: std::time::Duration) {
        // Verify streaming state is available
        if self.current_playback_source.is_none() {
            error!("Cannot seek: no streaming source active");
            return;
        }

        let prepared = match &self.current_prepared {
            Some(p) => p,
            None => {
                error!("Cannot seek: no current_prepared");
                return;
            }
        };

        let track_id = prepared.track_info.track_id.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;
        let buffer = prepared.buffer.clone();

        // Check for same-position seek (difference < 100ms)
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_diff = position.abs_diff(current_position);
        if position_diff < std::time::Duration::from_millis(100) {
            trace!(
                "Seek: Skipping seek to same position (difference: {:?} < 100ms)",
                position_diff
            );
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::SeekSkipped {
                    requested_position: position,
                    current_position,
                },
            );
            return;
        }

        // Abort old listeners immediately to prevent stale position ticks
        self.abort_current_listeners();

        // Preserve any staged gapless next track across the stream rebuild so
        // playback stays gapless after a seek. Removing it from the old chain
        // keeps the teardown below from cancelling its (still-running) decoder;
        // its source + fmt are re-staged into the new stream once it's built.
        let staged_next = if let Some(gapless) = &self.current_playback_source {
            gapless.lock().unwrap().take_next()
        } else {
            None
        };
        let staged_next: Option<(TrackStream, TrackFmt)> =
            staged_next.map(|(s, fmt)| (s, (*fmt).clone()));

        // Tear down old decoder, preserve buffer
        let cancel_token = self
            .current_prepared
            .as_ref()
            .map(|p| p.cancel_token.clone())
            .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(true)));
        let is_shared = self
            .current_prepared
            .as_ref()
            .is_some_and(|p| p.buffer_shared);
        Self::teardown_decoder_for_seek(
            &mut self.current_playback_source,
            &buffer,
            &cancel_token,
            &mut self.current_decoder_handle,
            is_shared,
        )
        .await;

        // Seek to the track's start plus the requested in-track position.
        let position_samples = (position.as_secs_f64() * sample_rate as f64) as u64;

        // Mint a fresh cancel token for the seek's decoder so the ready-watcher's
        // TrackReady is tied to this seek, and a later seek/switch supersedes it.
        let new_cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(prepared) = &mut self.current_prepared {
            prepared.cancel_token = new_cancel_token;
        }

        // Show buffering at the target immediately (the same Loading→Playing arc
        // the play path uses): the bar jumps to the seek position via Seeked
        // below, Loading covers the wait for the demanded window, and the
        // ready-watcher confirms Playing once audio flows. The decoder reads
        // immediately and the demand-driven fill fetches the seek target first,
        // so there's no fixed wait and no frozen-but-Playing bar.
        let prepared = self
            .current_prepared
            .as_ref()
            .expect("seek requires a current track");
        let decode = prepared.decode_params(position_samples);
        info!(
            "Seek: position {:?}, seek_to {}",
            position, decode.target_sample
        );
        let loading_duration_ms = pregap_adjusted_duration(prepared);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.clone(),
                    resolved: Some(LoadingTrack {
                        track_info: prepared.track_info.clone(),
                        duration_ms: loading_duration_ms,
                    }),
                },
            },
        );

        // Seek keeps the same current track, with the seek target as the new
        // stream's in-track offset.
        let fmt = prepared.track_fmt(position);

        if !self
            .start_decoder_and_watch(decode, fmt, sample_rate, channels, track_id.clone())
            .await
        {
            // The rebuilt stream failed. The preserved next track was taken out
            // of the old chain, so stop()'s teardown can't reach it — cancel it
            // here (otherwise its decoder parks forever filling a buffer with no
            // consumer). Then resolve the Loading we just emitted to Stopped via
            // stop(), the same hard-failure outcome the play path takes when
            // audio output can't start, so the bar doesn't hang in buffering.
            if let Some((next_source, _next_fmt)) = staged_next {
                next_source.cancel();
            }
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Couldn't restart audio output after the seek.",
                    ),
                },
            );
            self.stop().await;
            return;
        }

        // Re-stage the preserved gapless next track into the rebuilt stream so
        // post-seek auto-advance stays gapless without re-decoding. The init
        // above guarantees current_playback_source is set on the success
        // path — a silent skip here would leak the staged decoder, since
        // TrackStream doesn't cancel on drop.
        if let Some((next_source, next_fmt)) = staged_next {
            self.current_playback_source
                .as_ref()
                .expect("init_streaming succeeded above; gapless source must be set")
                .lock()
                .unwrap()
                .stage_next(next_source, next_fmt);
        }

        let raw_pos_ms = position.as_millis() as u64;
        self.emit_position_display(raw_pos_ms, track_id);
    }
}
