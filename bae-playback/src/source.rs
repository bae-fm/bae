//! The audio callback's source of samples.
//!
//! A `PlaybackSource` holds the current track's `TrackStream` plus an
//! optional pre-staged next track. When the current track is drained and the
//! producer has finished, and a next track is staged, the audio callback
//! advances to the next track *within the same callback* — no output stream
//! teardown — and signals the boundary so the service can promote its track
//! bookkeeping. With nothing staged (preview, end-of-queue, or a format change
//! that requires a stream rebuild) the source finishes at the current track's
//! end and the existing rebuild path runs.
//!
//! Only same-format tracks are staged (the persistent output stream is built
//! for one sample rate / channel count).

use crate::audio_output::{duration_millis, AudioEvent, AudioEventSender};
use crate::timeline::{StreamPosition, TrackTimeline};
use bae_audio::track_stream::TrackStream;
use bae_util::stall::Stall;
use std::sync::Arc;
use std::time::Instant;

/// Per-track context the service needs to label its playback events
/// (`PositionUpdate`, `TrackCompleted`, `DecodeStats`). Carried with every tick
/// and completion, so across a track boundary the audio callback emits the new
/// track's fmt with the new track's first tick and the service consults no
/// shared cell.
///
/// `track_id`/`timeline` are intrinsic to the track; `starts_at` is where in
/// the track's stream this decode began (a direct start's INDEX 01, a seek's
/// target, or the stream's start — where the next track in a chain always
/// begins).
#[derive(Debug, Clone)]
pub struct TrackFmt {
    pub track_id: String,
    pub timeline: TrackTimeline,
    pub starts_at: StreamPosition,
    /// Linear replay gain for this track, folded into the audio callback's
    /// volume multiply. `1.0` = no change. Per-track and swapped at the gapless
    /// boundary, so the loudness shifts discontinuously there — which is
    /// correct: each track carries its own normalization.
    pub replay_gain_linear: f32,
}

#[cfg(test)]
impl TrackFmt {
    /// A one-second track named `track_id`, played from its first sample at
    /// unity gain: the fmt a test needs when it cares about none of its parts.
    pub(crate) fn for_test(track_id: &str) -> Self {
        Self {
            track_id: track_id.to_string(),
            timeline: TrackTimeline::new(
                Some(std::time::Duration::from_secs(1)),
                std::time::Duration::ZERO,
            ),
            starts_at: StreamPosition::START,
            replay_gain_linear: 1.0,
        }
    }
}

/// Payload of the boundary signal. Carries the finishing track's identity +
/// decode stats so the service's boundary handler can emit `DecodeStats`
/// without reading shared state, plus the incoming track's identity for state
/// updates.
#[derive(Debug, Clone)]
pub struct TrackCrossing {
    pub finished_fmt: Arc<TrackFmt>,
    pub decode_error_count: u32,
    pub samples_decoded: u64,
    pub incoming_fmt: Arc<TrackFmt>,
}

/// What the audio callback pulls from: the current track's PCM source plus an
/// optional pre-staged next track to advance to in place at the current track's
/// end.
pub struct PlaybackSource {
    current: TrackStream,
    current_fmt: Arc<TrackFmt>,
    next: Option<(TrackStream, Arc<TrackFmt>)>,
    /// The current track's ring running dry, as the audio callback reports
    /// it. Distinct from the service-side `StarvationEpisode`, which is the
    /// fail-watchdog's per-track decode-progress baseline.
    starvation: Stall,
    /// Whether the current track's completion (drained, nothing staged after
    /// it) has already been reported via a `Completion` event. Reset when the
    /// current track changes — a gapless crossing or a `replace` — so each
    /// track that reaches the end of the source reports exactly once, even
    /// across a persistent output stream that plays many tracks through the
    /// same source.
    completion_reported: bool,
}

impl PlaybackSource {
    pub fn new(current: TrackStream, current_fmt: TrackFmt) -> Self {
        Self {
            current,
            current_fmt: Arc::new(current_fmt),
            next: None,
            starvation: Stall::default(),
            completion_reported: false,
        }
    }

    /// Stage the next track to play immediately after the current one. The
    /// caller is responsible for only staging a format-compatible track.
    pub fn stage_next(&mut self, next: TrackStream, next_fmt: TrackFmt) {
        self.next = Some((next, Arc::new(next_fmt)));
    }

    /// Replace the current track in place — the command-side sibling of the
    /// gapless crossing (`pull_samples`' in-callback swap). Cancels the outgoing
    /// track and any staged next, installs `next` as current, and resets the
    /// per-track bookkeeping (starvation span, completion reporting). The caller
    /// only replaces a format-compatible track: the output stream is built for
    /// one sample rate and channel count, and this swaps what its callback reads
    /// without rebuilding it. The cancel here is only the outgoing track's sink
    /// side (unpark + cancel flag); the caller cancels that decoder's token and
    /// wakes its byte buffers, exactly as a stream teardown would.
    pub fn replace(&mut self, next: TrackStream, next_fmt: TrackFmt) {
        self.current.cancel();
        if let Some((staged, _)) = self.next.take() {
            staged.cancel();
        }
        self.install_current(next, next_fmt);
    }

    pub fn replace_preserving_staged_next(&mut self, next: TrackStream, next_fmt: TrackFmt) {
        self.current.cancel();
        self.install_current(next, next_fmt);
    }

    fn install_current(&mut self, next: TrackStream, next_fmt: TrackFmt) {
        self.current = next;
        self.current_fmt = Arc::new(next_fmt);
        self.completion_reported = false;
        self.starvation.clear();
    }

    /// Whether a next track is currently staged.
    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }

    pub fn cancel_staged_next(&mut self) {
        if let Some((next, _)) = self.next.take() {
            next.cancel();
        }
    }

    /// Promote the staged successor before the current track drains, as for a
    /// manual Next command. The source keeps ownership of both streams across
    /// the swap; callers only learn whether a successor was present.
    pub fn promote_staged_next(&mut self) -> bool {
        let Some((next, next_fmt)) = self.next.take() else {
            return false;
        };
        self.current.cancel();
        self.current = next;
        self.current_fmt = next_fmt;
        self.completion_reported = false;
        self.starvation.clear();
        true
    }

    /// A position-tick event tagged with the current track's fmt. Built here
    /// so each audio-output implementation calls one method instead of
    /// re-assembling the tuple from the same accessors.
    pub fn position_event(&self) -> (Arc<TrackFmt>, StreamPosition) {
        (self.current_fmt.clone(), self.position())
    }

    /// A completion event tagged with the finishing track's fmt and its
    /// decode stats. Same bundling rationale as `position_event`. Also reused
    /// by `pull_samples` to assemble the boundary-crossing payload.
    pub fn completion_event(&self) -> (Arc<TrackFmt>, u32, u64) {
        (
            self.current_fmt.clone(),
            self.current.decode_error_count(),
            self.current.samples_decoded(),
        )
    }

    /// If the current track has drained (finished with nothing staged after it)
    /// and its completion hasn't been reported yet, mark it reported and return
    /// the completion event; `None` otherwise. The audio callback calls this
    /// once the ring runs dry, so the service's auto-advance fires exactly once
    /// per drained track — including on a persistent stream playing many tracks,
    /// since `replace`/crossing reset the latch.
    pub fn take_completion_event(&mut self) -> Option<(Arc<TrackFmt>, u32, u64)> {
        if self.is_finished() && !self.completion_reported {
            self.completion_reported = true;
            Some(self.completion_event())
        } else {
            None
        }
    }

    /// Cancel the current decoder and any staged next decoder.
    pub fn cancel(&self) {
        self.current.cancel();
        if let Some((next, _)) = &self.next {
            next.cancel();
        }
    }

    /// Pull samples for the audio callback, advancing across the track
    /// boundary when the current track is exhausted and a next track is
    /// staged.
    ///
    /// At most one boundary is crossed per call: only one track is staged at a
    /// time, and the next track's staging happens after the service handles
    /// this crossing.
    pub fn pull_samples(
        &mut self,
        output: &mut [f32],
        audio_events: &mut AudioEventSender,
    ) -> usize {
        let mut filled = self.current.pull_samples(output);

        if filled < output.len() && self.current.is_finished() {
            if let Some((next, next_fmt)) = self.next.take() {
                // Capture the finishing track's stats before swapping it out —
                // the service reports them at the boundary, since a track
                // advanced via the chain never reaches the completion path.
                let (finished_fmt, decode_error_count, samples_decoded) = self.completion_event();
                let crossing = TrackCrossing {
                    finished_fmt,
                    decode_error_count,
                    samples_decoded,
                    incoming_fmt: next_fmt.clone(),
                };
                self.current = next;
                self.current_fmt = next_fmt;
                // The incoming track hasn't drained; its completion is still to
                // come (the finishing track reported via the crossing, not a
                // Completion event).
                self.completion_reported = false;
                audio_events.push_required(AudioEvent::TrackCrossing(crossing));
                filled += self.current.pull_samples(&mut output[filled..]);
            }
        }

        self.record_starvation_if_needed(filled, audio_events);
        filled
    }

    fn record_starvation_if_needed(&mut self, filled: usize, audio_events: &mut AudioEventSender) {
        if filled > 0 {
            if let Some(starved) = self.starvation.ended(Instant::now()) {
                audio_events.push(AudioEvent::StarvationEnded {
                    fmt: self.current_fmt.clone(),
                    starved_ms: duration_millis(starved),
                    position: self.position(),
                    samples_decoded: self.current.samples_decoded(),
                    decode_errors: self.current.decode_error_count(),
                });
            }
            return;
        }

        if self.current.is_finished() {
            self.starvation.clear();
            return;
        }

        if let Some(starved) = self.starvation.waiting(Instant::now()) {
            audio_events.push(AudioEvent::Starved {
                fmt: self.current_fmt.clone(),
                starved_ms: duration_millis(starved),
                position: self.position(),
                producer_finished: self.current.producer_finished(),
                samples_decoded: self.current.samples_decoded(),
                decode_errors: self.current.decode_error_count(),
                has_next: self.next.is_some(),
            });
        }
    }

    /// Whether playback is fully finished: the current track is drained and
    /// finished and nothing is staged to follow it.
    pub fn is_finished(&self) -> bool {
        self.current.is_finished() && self.next.is_none()
    }

    /// Where the current track's stream has played to: where its decode began,
    /// plus what has played since.
    pub fn position(&self) -> StreamPosition {
        self.current_fmt.starts_at + self.current.position()
    }

    pub fn sample_rate(&self) -> u32 {
        self.current.sample_rate()
    }

    pub fn channels(&self) -> u32 {
        self.current.channels()
    }

    /// The current track's linear replay gain, read every callback to fold into
    /// the volume multiply. After a mid-buffer boundary crossing this returns
    /// the incoming track's gain (the same `current_fmt` the position path
    /// reads), so the new track's normalization takes effect at the boundary.
    pub fn current_replay_gain_linear(&self) -> f32 {
        self.current_fmt.replay_gain_linear
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
