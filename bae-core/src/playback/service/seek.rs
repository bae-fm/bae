use super::*;

impl PlaybackService {
    pub(super) async fn seek(&mut self, position: std::time::Duration) {
        // Verify streaming state is available
        if self.current_playback_source.is_none() {
            error!("Cannot seek: no streaming source active");
            return;
        }

        let prepared = match &self.current_prepared {
            Some(prepared) => prepared.clone(),
            None => {
                error!("Cannot seek: no current_prepared");
                return;
            }
        };
        let track_id = prepared.track_info.track_id.clone();

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

        teardown_decoder_for_seek(
            &mut self.current_playback_source,
            &prepared.buffer,
            &prepared.cancel_token,
            &mut self.current_decoder_handle,
        )
        .await;

        // Mint a fresh cancel token for the seek's decoder so the ready-watcher's
        // TrackReady is tied to this seek, and a later seek/switch supersedes it.
        let new_cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.current_prepared
            .as_mut()
            .expect("seek keeps the prepared track through decoder teardown")
            .cancel_token = new_cancel_token;

        // Show buffering at the target immediately (the same Loading→Playing arc
        // the play path uses): the bar jumps to the seek position via Seeked
        // below, Loading covers the wait for the demanded window, and the
        // ready-watcher confirms Playing once audio flows. The decoder reads
        // immediately and the demand-driven fill fetches the seek target first,
        // so there's no fixed wait and no frozen-but-Playing bar.
        let position_samples = (position.as_secs_f64() * prepared.sample_rate as f64) as u64;
        let decode = prepared.decode_params(position_samples);
        info!(
            "Seek: position {:?}, seek_to {}",
            position, decode.target_sample
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.clone(),
                    resolved: Some(LoadingTrack {
                        track_info: prepared.track_info.clone(),
                        duration_ms: pregap_adjusted_duration(&prepared),
                    }),
                },
            },
        );

        let fmt = prepared.track_fmt(position);
        if !self
            .start_decoder_and_watch(
                decode,
                fmt,
                prepared.sample_rate,
                prepared.channels,
                track_id.clone(),
            )
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
