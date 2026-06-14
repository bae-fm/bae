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

use crate::playback::track_stream::TrackStream;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

/// Per-track context the position/completion listener needs to label its
/// output (`PositionUpdate`, `TrackCompleted`, `DecodeStats`). Carried with
/// every tick and completion so the listener is a pure function of its input:
/// across a track boundary the audio callback emits the new track's fmt with
/// the new track's first tick, without the listener having to consult any
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
}

/// Payload of the boundary signal. Carries the finishing track's identity +
/// decode stats so the service's `TrackCrossed` handler can emit `DecodeStats`
/// without reading shared state, plus the incoming track's identity for state
/// updates. Only the track ids are surfaced — the boundary handler doesn't
/// need duration/pregap/offset, which live in the prepared track on the
/// service side.
#[derive(Debug, Clone)]
pub struct TrackCrossing {
    pub finished_track_id: String,
    pub decode_error_count: u32,
    pub samples_decoded: u64,
    pub incoming_track_id: String,
}

/// What the audio callback pulls from: the current track's PCM source plus an
/// optional pre-staged next track to advance to in place at the current track's
/// end.
pub struct PlaybackSource {
    current: TrackStream,
    current_fmt: Arc<TrackFmt>,
    next: Option<(TrackStream, Arc<TrackFmt>)>,
    /// Fired once each time playback crosses from `current` into `next`,
    /// carrying the finishing track's stats so the service can report them
    /// (a track advanced via the chain never reaches the completion path). An
    /// async (tokio) sender so it can be sent from the audio callback yet
    /// drain via a cancellable async receiver — a blocking receiver would
    /// wedge runtime shutdown, since this sender outlives any single stream.
    boundary_tx: UnboundedSender<TrackCrossing>,
}

impl PlaybackSource {
    /// A chained source. `boundary_tx` is signalled on every track crossing.
    /// For single-track use (preview), pass a sender whose receiver is
    /// dropped — the sends become no-ops and `stage_next` is simply not called.
    pub fn new(
        current: TrackStream,
        current_fmt: TrackFmt,
        boundary_tx: UnboundedSender<TrackCrossing>,
    ) -> Self {
        Self {
            current,
            current_fmt: Arc::new(current_fmt),
            next: None,
            boundary_tx,
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
    pub fn pull_samples(&mut self, output: &mut [f32]) -> usize {
        let mut filled = self.current.pull_samples(output);

        if filled < output.len() && self.current.is_finished() {
            if let Some((next, next_fmt)) = self.next.take() {
                // Capture the finishing track's stats before swapping it out —
                // the service reports them at the boundary, since a track
                // advanced via the chain never reaches the completion path.
                let crossing = TrackCrossing {
                    finished_track_id: self.current_fmt.track_id.clone(),
                    decode_error_count: self.current.decode_error_count(),
                    samples_decoded: self.current.samples_decoded(),
                    incoming_track_id: next_fmt.track_id.clone(),
                };
                self.current = next;
                self.current_fmt = next_fmt;
                // Notify the service to promote track bookkeeping. Matches
                // the existing completion-signal pattern (a send from the
                // audio callback); fired only at track boundaries, not per
                // callback. A failed send means the receiver was dropped
                // (service shutting down) — log it once, the boundary's stats
                // are then forfeit for this crossing.
                if self.boundary_tx.send(crossing).is_err() {
                    warn!("Failed to send track boundary signal");
                }
                filled += self.current.pull_samples(&mut output[filled..]);
            }
        }

        filled
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::track_stream::create_track_stream_pair;

    fn fmt(track_id: &str) -> TrackFmt {
        TrackFmt {
            track_id: track_id.to_string(),
            duration_ms: 1000,
            pregap_ms: None,
            position_offset: Duration::ZERO,
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

        let (boundary_tx, mut boundary_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut source = PlaybackSource::new(src1, fmt("t1"), boundary_tx);
        source.stage_next(src2, fmt("t2"));
        assert!(source.has_next());
        assert!(!source.is_finished());
        assert_eq!(source.position_event().0.track_id, "t1");

        // One pull spans the boundary: track 1 then track 2, contiguous.
        let mut out = [0.0f32; 8];
        let n = source.pull_samples(&mut out);
        assert_eq!(n, 5, "should yield track 1 (3) + track 2 (2)");
        assert_eq!(&out[..5], &[1.0, 2.0, 3.0, 10.0, 11.0]);

        // The boundary fired exactly once, carrying t1's finishing stats and
        // both tracks' identities.
        let crossing = boundary_rx.try_recv().unwrap();
        assert_eq!(crossing.finished_track_id, "t1");
        assert_eq!(crossing.incoming_track_id, "t2");
        assert_eq!(crossing.decode_error_count, 7);
        assert_eq!(crossing.samples_decoded, 3);
        assert!(boundary_rx.try_recv().is_err());

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

        let (boundary_tx, mut boundary_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut source = PlaybackSource::new(src, fmt("t1"), boundary_tx);

        let mut out = [0.0f32; 8];
        assert_eq!(source.pull_samples(&mut out), 2);
        assert_eq!(&out[..2], &[1.0, 2.0]);
        assert!(source.is_finished());
        assert!(
            boundary_rx.try_recv().is_err(),
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

        let (boundary_tx, mut boundary_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut source = PlaybackSource::new(src1, fmt("t1"), boundary_tx);
        source.stage_next(src2, fmt("t2"));
        let taken = source.take_next();
        assert!(taken.is_some());
        let (_taken_src, taken_fmt) = taken.unwrap();
        assert_eq!(taken_fmt.track_id, "t2");
        assert!(!source.has_next());

        let mut out = [0.0f32; 8];
        assert_eq!(source.pull_samples(&mut out), 1);
        assert!(source.is_finished());
        assert!(boundary_rx.try_recv().is_err());
    }
}
