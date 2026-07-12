use super::*;

impl PlaybackService {
    pub(super) async fn seek(&mut self, position: std::time::Duration) {
        // Drain first so a pending gapless crossing is applied before we read the
        // current track — a seek arriving right at a boundary then rebuilds the
        // track the callback already advanced into, not the finishing one.
        self.drain_current_audio_events().await;

        // Seek only operates on an active track: take it out to tear its decoder
        // down and rebuild. A Stopped or still-resolving (Loading) slot has no
        // stream to rebuild and goes back untouched. Nothing observes the Stopped
        // slot in between — no emission, and the serial command loop can't
        // interleave anything until seek returns.
        let cur = match std::mem::replace(&mut self.slot, PlaybackSlot::Stopped) {
            PlaybackSlot::Active(cur) => cur,
            other => {
                self.slot = other;
                error!("Cannot seek: no active track");
                return;
            }
        };
        let track_id = cur.prepared.track_info.track_id.clone();

        // Same-position seek (difference < 100ms): put the track back, refresh the
        // display, no rebuild.
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
            self.slot = PlaybackSlot::Active(cur);
            self.emit_position_display(position.as_millis() as u64, track_id);
            return;
        }

        let generation = self.next_load_generation();
        let CurrentTrack {
            prepared,
            decoder: old_decoder,
            phase,
        } = cur;

        // Where the rebuilt load should land: seek preserves the current
        // play/pause intent. A completed track resumes audibly at the seek
        // target (rather than staying silently frozen with a Stopped atomic).
        let target = match phase {
            TrackPhase::Playing | TrackPhase::Completed => PlayTarget::Playing,
            TrackPhase::Paused(pause) => PlayTarget::Paused(pause),
            TrackPhase::Loading { target, .. } => target,
        };

        let old_buffers: Vec<_> = prepared
            .segments
            .iter()
            .map(|segment| segment.buffer.clone())
            .collect();

        // Preserve any staged gapless next track across the rebuild so playback
        // stays gapless after the seek. Taking it out of the persistent source now
        // keeps the `replace` below from cancelling its (still-running) decoder;
        // its source + fmt are re-staged after the new decoder attaches.
        let staged_next: Option<(TrackStream, TrackFmt)> = self
            .output
            .as_ref()
            .and_then(|o| o.source.lock().unwrap().take_next())
            .map(|(s, fmt)| (s, (*fmt).clone()));

        // The same Loading→ready arc the play path uses: the bar jumps to the seek
        // position via the `Seeked` below, Loading covers the wait for the demanded
        // window, and the ready-watcher confirms the target once audio flows.
        // Projecting Stopped onto the atomic silences the callback while the new
        // decoder fills, so nothing leaks from the old ring before the swap.
        self.slot = PlaybackSlot::Loading {
            track_id: track_id.clone(),
            resolved: Some(LoadingTrack::from_prepared(&prepared)),
        };
        self.sync_audio_state();
        self.emit_state();

        let position_samples = (position.as_secs_f64() * prepared.sample_rate as f64) as u64;
        let decode = prepared.decode_params(position_samples, true);
        let fmt = prepared.track_fmt(position);
        info!("Seek: position {:?}", position);

        // Spawn the seek's fresh decoder over the same byte buffers and swap it
        // into the persistent source (same format → `replace`). The old decoder is
        // still running against the same buffers; two readers on one sparse buffer
        // is supported, and the new decoder is spawned before the old is joined to
        // minimize the silent window.
        let new_decoder = match self
            .start_decoder_and_watch(&prepared, decode, fmt, generation)
            .await
        {
            Ok(decoder) => decoder,
            Err(_) => {
                // Only reachable on a format-change build, which a same-track seek
                // never hits — but handle it anyway. The preserved next track is
                // out of the source, so `stop()`'s teardown can't reach it: cancel
                // it, and the old decoder, here.
                if let Some((next_source, _next_fmt)) = staged_next {
                    next_source.cancel();
                }
                old_decoder
                    .cancel_token
                    .store(true, std::sync::atomic::Ordering::Release);
                for buffer in &old_buffers {
                    buffer.wake_readers();
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
        };

        // Re-stage the preserved next track into the persistent source (the same
        // one, replaced in place above), so post-seek auto-advance stays gapless
        // without re-decoding it.
        if let Some((next_source, next_fmt)) = staged_next {
            if let Some(out) = &self.output {
                out.source.lock().unwrap().stage_next(next_source, next_fmt);
            }
        }

        // Now cancel + join the old decoder so the reused byte buffers are free and
        // the old thread is gone. `replace` already cancelled its sink, so
        // `cancel_and_join_decoder`'s token + buffer wake reaches it wherever it is
        // parked.
        let TrackDecoder {
            handle: old_handle,
            cancel_token: old_cancel_token,
        } = old_decoder;
        cancel_and_join_decoder(&old_cancel_token, &old_buffers, old_handle).await;

        // Reassemble the current track. The phase stays Loading (with this seek's
        // generation and target) until the ready-watcher's TrackReady resolves it.
        self.install_active_track(
            prepared,
            new_decoder,
            TrackPhase::Loading { generation, target },
        );

        let raw_pos_ms = position.as_millis() as u64;
        self.emit_position_display(raw_pos_ms, track_id);
    }
}
