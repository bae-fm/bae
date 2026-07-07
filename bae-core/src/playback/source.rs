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

use crate::playback::audio_output::{duration_millis, AudioEvent, AudioEventSender};
use crate::playback::track_stream::TrackStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

const STARVATION_LOG_AFTER: Duration = Duration::from_millis(250);
const STARVATION_LOG_EVERY: Duration = Duration::from_secs(1);

/// Per-track context the service needs to label its playback events
/// (`PositionUpdate`, `TrackCompleted`, `DecodeStats`). Carried with every
/// tick and completion so event handling is a pure function of its input:
/// across a track boundary the audio callback emits the new track's fmt with
/// the new track's first tick, without the service having to consult any
/// shared cell.
///
/// `track_id`/`duration_ms`/`pregap_ms` are intrinsic to the track;
/// `position_offset` is the in-track time at which this stream began
/// (non-zero only after a seek — the next track in a chain always starts at
/// zero).
#[derive(Debug, Clone)]
pub struct TrackFmt {
    pub track_id: String,
    pub duration_ms: u64,
    pub pregap_ms: Option<i64>,
    pub position_offset: Duration,
    /// Linear replay gain for this track, folded into the audio callback's
    /// volume multiply. `1.0` = no change. Per-track and swapped at the gapless
    /// boundary, so the loudness shifts discontinuously there — which is
    /// correct: each track carries its own normalization.
    pub replay_gain_linear: f32,
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
    starvation_started: Option<Instant>,
    last_starvation_log: Option<Instant>,
}

impl PlaybackSource {
    pub fn new(current: TrackStream, current_fmt: TrackFmt) -> Self {
        Self {
            current,
            current_fmt: Arc::new(current_fmt),
            next: None,
            starvation_started: None,
            last_starvation_log: None,
        }
    }

    /// Stage the next track to play immediately after the current one. The
    /// caller is responsible for only staging a format-compatible track.
    pub fn stage_next(&mut self, next: TrackStream, next_fmt: TrackFmt) {
        self.next = Some((next, Arc::new(next_fmt)));
    }

    /// Whether a next track is currently staged.
    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }

    /// Remove and return the staged next track, used to invalidate the chain
    /// on user actions (Next/Previous/Seek) and queue mutations.
    pub fn take_next(&mut self) -> Option<(TrackStream, Arc<TrackFmt>)> {
        self.next.take()
    }

    /// A position-tick event tagged with the current track's fmt. Built here
    /// so each audio-output implementation calls one method instead of
    /// re-assembling the tuple from the same accessors.
    pub fn position_event(&self) -> (Arc<TrackFmt>, Duration) {
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
                audio_events.push_required(AudioEvent::TrackCrossing(crossing));
                filled += self.current.pull_samples(&mut output[filled..]);
            }
        }

        self.record_starvation_if_needed(filled, audio_events);
        filled
    }

    fn record_starvation_if_needed(&mut self, filled: usize, audio_events: &mut AudioEventSender) {
        if filled > 0 {
            if let Some(started) = self.starvation_started.take() {
                let starved = started.elapsed();
                if starved >= STARVATION_LOG_AFTER {
                    audio_events.push(AudioEvent::StarvationEnded {
                        fmt: self.current_fmt.clone(),
                        starved_ms: duration_millis(starved),
                        position_ms: duration_millis(self.position()),
                        samples_decoded: self.current.samples_decoded(),
                        decode_errors: self.current.decode_error_count(),
                    });
                }
                self.last_starvation_log = None;
            }
            return;
        }

        if self.current.is_finished() {
            self.starvation_started = None;
            self.last_starvation_log = None;
            return;
        }

        let now = Instant::now();
        let started = *self.starvation_started.get_or_insert(now);
        let starved = now.duration_since(started);
        if starved >= STARVATION_LOG_AFTER
            && self
                .last_starvation_log
                .is_none_or(|last| now.duration_since(last) >= STARVATION_LOG_EVERY)
        {
            audio_events.push(AudioEvent::Starved {
                fmt: self.current_fmt.clone(),
                starved_ms: duration_millis(starved),
                position_ms: duration_millis(self.position()),
                producer_finished: self.current.producer_finished(),
                samples_decoded: self.current.samples_decoded(),
                decode_errors: self.current.decode_error_count(),
                has_next: self.next.is_some(),
            });
            self.last_starvation_log = Some(now);
        }
    }

    /// Whether playback is fully finished: the current track is drained and
    /// finished and nothing is staged to follow it.
    pub fn is_finished(&self) -> bool {
        self.current.is_finished() && self.next.is_none()
    }

    pub fn position(&self) -> Duration {
        self.current.position()
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
mod tests {
    use super::*;
    use crate::playback::audio_output::audio_event_channel;
    use crate::playback::track_stream::create_track_stream_pair;

    fn fmt(track_id: &str) -> TrackFmt {
        TrackFmt {
            track_id: track_id.to_string(),
            duration_ms: 1000,
            pregap_ms: None,
            position_offset: Duration::ZERO,
            replay_gain_linear: 1.0,
        }
    }

    fn fmt_gain(track_id: &str, replay_gain_linear: f32) -> TrackFmt {
        TrackFmt {
            replay_gain_linear,
            ..fmt(track_id)
        }
    }

    /// A staged next track plays immediately after the current one: a single
    /// source advances across the boundary in place (no rebuild), emits the
    /// finishing track's identity + decode stats, and reports finished only
    /// once both tracks are drained.
    #[test]
    fn advances_into_staged_next_track() {
        let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
        let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);

        // Track 1: three samples + decode stats, then EOF.
        assert_eq!(sink1.push_samples(&[1.0, 2.0, 3.0]), 3);
        sink1.set_decode_error_count(7);
        sink1.set_samples_decoded(3);
        sink1.mark_finished();
        // Track 2: two samples, then EOF.
        assert_eq!(sink2.push_samples(&[10.0, 11.0]), 2);
        sink2.mark_finished();

        let (mut audio_tx, mut audio_rx) = audio_event_channel();
        let mut source = PlaybackSource::new(src1, fmt("t1"));
        source.stage_next(src2, fmt("t2"));
        assert!(source.has_next());
        assert!(!source.is_finished());
        assert_eq!(source.position_event().0.track_id, "t1");

        // One pull spans the boundary: track 1 then track 2, contiguous.
        let mut out = [0.0f32; 8];
        let n = source.pull_samples(&mut out, &mut audio_tx);
        assert_eq!(n, 5, "should yield track 1 (3) + track 2 (2)");
        assert_eq!(&out[..5], &[1.0, 2.0, 3.0, 10.0, 11.0]);

        // The boundary fired exactly once, carrying t1's finishing stats and
        // both tracks' identities.
        let crossing = match audio_rx.pop().unwrap() {
            AudioEvent::TrackCrossing(crossing) => crossing,
            event => panic!("expected crossing event, got {event:?}"),
        };
        assert_eq!(crossing.finished_fmt.track_id, "t1");
        assert_eq!(crossing.incoming_fmt.track_id, "t2");
        assert_eq!(crossing.decode_error_count, 7);
        assert_eq!(crossing.samples_decoded, 3);
        assert!(audio_rx.pop().is_none());

        // After the crossing the fmt advanced to the new track.
        assert_eq!(source.position_event().0.track_id, "t2");

        // Successor consumed; with both tracks drained the source is finished.
        assert!(!source.has_next());
        assert!(source.is_finished());
    }

    /// With nothing staged, the source finishes at the current track's end and
    /// never advances or signals a boundary.
    #[test]
    fn finishes_without_a_staged_next() {
        let (mut sink, src, _r) = create_track_stream_pair(44100, 1);
        assert_eq!(sink.push_samples(&[1.0, 2.0]), 2);
        sink.mark_finished();

        let (mut audio_tx, mut audio_rx) = audio_event_channel();
        let mut source = PlaybackSource::new(src, fmt("t1"));

        let mut out = [0.0f32; 8];
        assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 2);
        assert_eq!(&out[..2], &[1.0, 2.0]);
        assert!(source.is_finished());
        assert!(
            audio_rx.pop().is_none(),
            "no boundary without a staged next"
        );
    }

    /// `take_next` un-stages the successor (used to invalidate the chain on
    /// user actions); the source then finishes at the current track's end with
    /// no boundary.
    #[test]
    fn take_next_unstages_the_successor() {
        let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
        let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);
        assert_eq!(sink1.push_samples(&[1.0]), 1);
        sink1.mark_finished();
        assert_eq!(sink2.push_samples(&[10.0]), 1);
        sink2.mark_finished();

        let (mut audio_tx, mut audio_rx) = audio_event_channel();
        let mut source = PlaybackSource::new(src1, fmt("t1"));
        source.stage_next(src2, fmt("t2"));
        let taken = source.take_next();
        assert!(taken.is_some());
        let (_taken_src, taken_fmt) = taken.unwrap();
        assert_eq!(taken_fmt.track_id, "t2");
        assert!(!source.has_next());

        let mut out = [0.0f32; 8];
        assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 1);
        assert!(source.is_finished());
        assert!(audio_rx.pop().is_none());
    }

    /// Replay gain is per-track, so crossing into the staged next track swaps in
    /// its gain — the loudness normalization the audio callback folds into its
    /// volume multiply changes discontinuously at the boundary, which is the
    /// whole reason the field lives on the fmt rather than the source.
    #[test]
    fn replay_gain_flips_at_the_gapless_boundary() {
        let (mut sink1, src1, _r1) = create_track_stream_pair(44100, 1);
        let (mut sink2, src2, _r2) = create_track_stream_pair(44100, 1);
        assert_eq!(sink1.push_samples(&[1.0, 2.0]), 2);
        sink1.mark_finished();
        assert_eq!(sink2.push_samples(&[3.0]), 1);
        sink2.mark_finished();

        let (mut audio_tx, _audio_rx) = audio_event_channel();
        let mut source = PlaybackSource::new(src1, fmt_gain("t1", 0.5));
        source.stage_next(src2, fmt_gain("t2", 2.0));

        // Before the crossing the callback reads the current track's gain.
        assert_eq!(source.current_replay_gain_linear(), 0.5);

        // One pull spans the boundary into t2.
        let mut out = [0.0f32; 8];
        assert_eq!(source.pull_samples(&mut out, &mut audio_tx), 3);

        // After the crossing the incoming track's gain takes effect.
        assert_eq!(source.current_replay_gain_linear(), 2.0);
    }

    /// `cancel` stops both the current decoder and the staged-next decoder, so a
    /// user action that invalidates the chain doesn't leave the preloaded track's
    /// decoder running. Observed through the retained sinks, which share the
    /// cancel flag with their streams.
    #[test]
    fn cancel_cancels_current_and_staged_next() {
        let (sink1, src1, _r1) = create_track_stream_pair(44100, 1);
        let (sink2, src2, _r2) = create_track_stream_pair(44100, 1);

        let mut source = PlaybackSource::new(src1, fmt("t1"));
        source.stage_next(src2, fmt("t2"));

        assert!(!sink1.is_cancelled());
        assert!(!sink2.is_cancelled());

        source.cancel();

        assert!(sink1.is_cancelled(), "the current track is cancelled");
        assert!(sink2.is_cancelled(), "the staged next track is cancelled");
    }
}
