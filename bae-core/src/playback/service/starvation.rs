//! The starvation watchdog: escalates a decoder that has stalled forever to a
//! `PlaybackError`, so it fails loud instead of logging a once-per-second
//! `Starved` warning behind a frozen position bar (the failure shape of a
//! decoder blocked on a byte buffer that will never produce — a cancelled
//! fill task, a reader that's never woken).
//!
//! A `Starved` event alone doesn't distinguish a genuine stall from an
//! ordinary slow producer (a cloud fetch still landing bytes): both look like
//! "the ring is empty" from the callback's side. `samples_decoded` — live off
//! every push into the ring — is what tells them apart: a slow producer keeps
//! advancing it even while starved; a stalled one doesn't advance it at all.

use super::*;

/// How long a starvation episode with zero decode progress must persist
/// before it's treated as a genuine stall — a decoder wedged for good, not a
/// producer that's merely slow — and escalated to a `PlaybackError`.
const STARVATION_FAIL_AFTER_MS: u64 = 30_000;

/// One escalation episode: the `samples_decoded` count observed when the
/// current starvation began, for the track it began on. As long as that count
/// keeps advancing, the producer is alive; once it stalls flat past
/// `STARVATION_FAIL_AFTER_MS`, playback has stalled for good.
pub(super) struct StarvationEpisode {
    track_id: String,
    samples_decoded_at_start: u64,
}

impl PlaybackService {
    /// Feed one `Starved` event (`producer_finished == false`) to the
    /// watchdog. Establishes a fresh episode baseline on the first
    /// observation of a track's stall (or re-baselines whenever
    /// `samples_decoded` has advanced past the current baseline — the
    /// producer is alive); only a flat baseline that persists past the fail
    /// threshold escalates.
    pub(super) async fn handle_starvation(
        &mut self,
        track_id: &str,
        samples_decoded: u64,
        starved_ms: u64,
    ) {
        // Onset: the first starvation observation for this track (no episode yet,
        // or the prior one was for a different track). A re-baseline of the same
        // track's ongoing stall is not a fresh onset and ships nothing.
        let is_onset = !matches!(&self.starvation_episode, Some(e) if e.track_id == track_id);
        if is_onset {
            let position_ms = self
                .current_position_shared
                .lock()
                .unwrap()
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.record_telemetry(TelemetryEvent::PlaybackStarved {
                track_id: LocalId(track_id.to_string()),
                position_ms,
            });
        }

        let stalled = matches!(
            &self.starvation_episode,
            Some(episode)
                if episode.track_id == track_id
                    && samples_decoded <= episode.samples_decoded_at_start
        );

        if !stalled {
            // A fresh track's first starvation, or the producer advanced
            // since the episode began: (re)baseline at the current count.
            self.starvation_episode = Some(StarvationEpisode {
                track_id: track_id.to_string(),
                samples_decoded_at_start: samples_decoded,
            });
            return;
        }

        if starved_ms >= STARVATION_FAIL_AFTER_MS {
            error!(
                track_id,
                starved_ms, "starvation exceeded the fail threshold with no decode progress"
            );
            self.telemetry_playback_failed(PlaybackOperation::Starvation);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(format!(
                        "Playback stalled on track {track_id} for {starved_ms}ms with no progress"
                    )),
                },
            );
            self.starvation_episode = None;
            self.stop().await;
        }
    }

    /// Reset the watchdog clock: no starvation episode is in progress. Called
    /// on `StarvationEnded`, on every `Position` tick, on any track install,
    /// and on `stop()` — anywhere the current track is either flowing
    /// normally or gone.
    pub(super) fn reset_starvation_episode(&mut self) {
        self.starvation_episode = None;
    }
}
