//! Track completion waits until buffered receiver audio has played.
use super::*;

/// A drained decoder whose last samples are still queued at the receiver.
pub(super) struct AudibleCompletion {
    pub(super) fmt: Arc<TrackFmt>,
    pub(super) errors: u32,
    pub(super) samples: u64,
    pub(super) audible_at: chrono::DateTime<chrono::Utc>,
}

impl PlaybackService {
    pub(super) async fn handle_completion_event(
        &mut self,
        fmt: Arc<TrackFmt>,
        error_count: u32,
        samples_decoded: u64,
    ) {
        self.finish_listening_audio(fmt.clone(), samples_decoded);
        self.finish_listening().await;
        if let Some(latency) = self.renderer.airplay_latency() {
            self.audible_completion = Some(AudibleCompletion {
                fmt,
                errors: error_count,
                samples: samples_decoded,
                audible_at: self.clock.now()
                    + chrono::Duration::from_std(latency)
                        .expect("receiver latency fits a clock deadline"),
            });
            if let PlaybackSlot::Active(cur) = &mut self.slot {
                cur.phase = TrackPhase::Completed;
            }
            self.sync_audio_state();
            return;
        }
        self.publish_completion(fmt, error_count, samples_decoded);
    }

    pub(super) fn publish_audible_completion(&mut self) {
        let waiting = matches!(&self.slot, PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Completed));
        if waiting
            && self
                .audible_completion
                .as_ref()
                .is_some_and(|completion| completion.audible_at <= self.clock.now())
        {
            let completion = self
                .audible_completion
                .take()
                .expect("the audible completion is present");
            self.publish_completion(completion.fmt, completion.errors, completion.samples);
        }
    }

    pub(super) fn publish_completion(
        &mut self,
        fmt: Arc<TrackFmt>,
        error_count: u32,
        samples_decoded: u64,
    ) {
        info!(
            "Track completed: {} ({} decode errors, {} samples)",
            fmt.track_id, error_count, samples_decoded
        );
        self.record_telemetry(TelemetryEvent::TrackCompleted {
            track_id: LocalId(fmt.track_id.clone()),
            decode_errors: error_count as u64,
        });
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::TrackCompleted {
                track_id: fmt.track_id.clone(),
            },
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::DecodeStats {
                track_id: fmt.track_id.clone(),
                error_count,
                samples_decoded,
            },
        );
        // The track drained: mark the phase Completed. The persistent output stays
        // live because AutoAdvance and the side-pause decision still read the
        // source; the audio callback already flipped the atomic to Stopped (which
        // `sync_audio_state` below confirms), so it emits nothing further until
        // the source is replaced.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Completed;
        }
        self.sync_audio_state();
    }
}
