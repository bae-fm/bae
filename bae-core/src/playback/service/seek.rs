use super::*;

impl PlaybackService {
    pub(super) async fn seek(&mut self, position: std::time::Duration) {
        // Drain first so a pending gapless crossing is applied before we read the
        // current track — a seek arriving right at a boundary then rebuilds the
        // track the callback already advanced into, not the finishing one.
        self.drain_current_audio_events().await;

        // Seek only operates on an active track. Take it out to tear down its
        // decoder and rebuild; a Stopped or still-resolving (Loading) slot has no
        // stream to rebuild and is put back untouched. While the slot is Stopped
        // here nothing observes it: no emission, and the serial loop can't
        // interleave a command until seek returns.
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
            mut prepared,
            stream,
            source,
            decoder_handle,
            audio_events,
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

        // Drop the old stream and its event receiver — the rebuild replaces them.
        drop(stream);
        drop(audio_events);

        let old_cancel_token = prepared.cancel_token.clone();
        let old_buffers: Vec<_> = prepared
            .segments
            .iter()
            .map(|segment| segment.buffer.clone())
            .collect();
        // Mint a fresh cancel token for the seek's decoder so cancelling the old
        // decoder below doesn't cancel the new one; staleness is decided by
        // generation, not by this token.
        prepared.cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Preserve any staged gapless next track across the rebuild so playback
        // stays gapless after the seek. Removing it from the old chain keeps the
        // teardown below from cancelling its (still-running) decoder; its source +
        // fmt are re-staged into the new stream once it's built.
        let staged_next: Option<(TrackStream, TrackFmt)> = source
            .lock()
            .unwrap()
            .take_next()
            .map(|(s, fmt)| (s, (*fmt).clone()));

        let mut source_opt = Some(source);
        let mut decoder_opt = Some(decoder_handle);
        teardown_decoder_for_seek(
            &mut source_opt,
            &old_buffers,
            &old_cancel_token,
            &mut decoder_opt,
        )
        .await;

        // Show buffering at the target immediately (the same Loading→ready arc the
        // play path uses): the bar jumps to the seek position via Seeked below,
        // Loading covers the wait for the demanded window, and the ready-watcher
        // confirms the target once audio flows. The decoder reads immediately and
        // the demand-driven fill fetches the seek target first, so there's no
        // fixed wait and no frozen-but-playing bar.
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

        let started = match self
            .start_decoder_and_watch(&prepared, decode, fmt, generation)
            .await
        {
            Ok(started) => started,
            Err(_) => {
                // The rebuilt stream failed. The preserved next track was taken
                // out of the old chain, so stop()'s teardown can't reach it —
                // cancel it here (otherwise its decoder parks forever filling a
                // buffer with no consumer). Then resolve the Loading we just
                // emitted to Stopped via stop(), the same hard-failure outcome the
                // play path takes when audio output can't start.
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
        };

        // Re-stage the preserved gapless next track into the rebuilt stream's
        // source before it becomes current, so post-seek auto-advance stays
        // gapless without re-decoding.
        if let Some((next_source, next_fmt)) = staged_next {
            started
                .parts
                .source
                .lock()
                .unwrap()
                .stage_next(next_source, next_fmt);
        }

        // Reassemble the current track. The phase stays Loading (with this seek's
        // generation and target) until the ready-watcher's TrackReady resolves it.
        self.install_active_track(
            prepared,
            started,
            TrackPhase::Loading { generation, target },
        );

        let raw_pos_ms = position.as_millis() as u64;
        self.emit_position_display(raw_pos_ms, track_id);
    }
}
