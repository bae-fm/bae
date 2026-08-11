use super::*;

impl PlaybackService {
    pub(super) async fn seek(&mut self, position: std::time::Duration) {
        // While playing remotely, the device owns the timeline: seek it, refresh
        // the position display, and skip the whole local decoder-rebuild path.
        if self.renderer.is_remote() {
            self.renderer.seek_remote(position);
            *self.current_position_shared.lock().unwrap() = Some(position);
            if let Some(track_id) = self.current_track_id().map(str::to_string) {
                self.emit_position_display(position.as_millis() as u64, track_id);
            }
            return;
        }

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

        // On AirPlay, decode is local so the rebuild below re-fills the sink at the
        // new position; FLUSH the receiver's stale buffer now and re-anchor once
        // the rebuilt stream is installed.
        self.renderer.flush_airplay();

        // Time the rebuild: from here to the rebuilt stream being installed.
        let seek_started_at = std::time::Instant::now();
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
        match self
            .start_decoder_and_install(
                prepared,
                decode,
                fmt,
                generation,
                StagedNextOnReplace::Preserve,
                TrackPhase::Loading { generation, target },
                Some((old_decoder, old_buffers)),
            )
            .await
        {
            Ok(()) => {}
            Err(_) => {
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
        }

        self.record_telemetry(TelemetryEvent::SeekCompleted {
            track_id: LocalId(track_id.clone()),
            wait: seek_started_at.elapsed(),
        });

        let raw_pos_ms = position.as_millis() as u64;
        self.emit_position_display(raw_pos_ms, track_id);

        self.renderer.reanchor_airplay();
    }
}
