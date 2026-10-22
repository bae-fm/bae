//! A track's two timelines, and the one conversion between them.
//!
//! A track's *stream* starts at its first sample: INDEX 00, the start of its
//! pregap when it has one — stored audio, or the silence a CUE `PREGAP`
//! generates. The decoder counts in it, seeks land in it, the resume cache
//! saves it, and a remote renderer is served it. The *track* starts at INDEX
//! 01, after the pregap: the stored duration runs from there, and the player
//! shows time from there, negative while the pregap plays.
//!
//! [`StreamPosition`] and [`TrackTime`] keep the two apart, and
//! [`TrackTimeline`] is the only way from one to the other, so display,
//! progress, seeking and the restart threshold agree on where the track starts.

use std::time::Duration;

/// A position in a track's stream: time from its first sample (INDEX 00).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamPosition(Duration);

impl StreamPosition {
    /// The stream's first sample.
    pub const START: Self = Self(Duration::ZERO);

    /// The position `since_start` after the stream's first sample.
    pub const fn from_duration(since_start: Duration) -> Self {
        Self(since_start)
    }

    pub const fn from_millis(ms: u64) -> Self {
        Self(Duration::from_millis(ms))
    }

    /// Time from the stream's first sample.
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    pub fn as_millis(self) -> u64 {
        self.0.as_millis() as u64
    }

    /// The frame this position falls on at `sample_rate`.
    pub fn frames(self, sample_rate: u32) -> u64 {
        (self.0.as_secs_f64() * f64::from(sample_rate)) as u64
    }

    /// This position moved `earlier` back, stopping at the stream's start.
    pub fn saturating_sub(self, earlier: Duration) -> Self {
        Self(self.0.saturating_sub(earlier))
    }

    /// The time between this position and `other`.
    pub fn abs_diff(self, other: Self) -> Duration {
        self.0.abs_diff(other.0)
    }
}

/// `elapsed` played on from a position.
impl std::ops::Add<Duration> for StreamPosition {
    type Output = Self;

    fn add(self, elapsed: Duration) -> Self {
        Self(self.0 + elapsed)
    }
}

/// Time into a track from its start (INDEX 01), in milliseconds: what the
/// player shows. Negative while the pregap plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackTime(i64);

impl TrackTime {
    /// The track's start, INDEX 01.
    pub const START: Self = Self(0);

    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    /// The time `since_start` after INDEX 01.
    pub fn from_duration(since_start: std::time::Duration) -> Self {
        Self(i64::try_from(since_start.as_millis()).expect("track time exceeds i64 milliseconds"))
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }
}

/// Where a track sits in its stream: how long its pregap runs before INDEX 01,
/// and how long it runs from there when that is known.
///
/// A track's length is unknown when the library stores none for it. Its
/// position still runs, but nothing holds it at an end, the seek bar has
/// nothing to fill toward, and no fill points at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackTimeline {
    pregap: Duration,
    duration: Option<Duration>,
}

impl TrackTimeline {
    /// The timeline of a track that runs `duration` from INDEX 01 (`None` when
    /// its length is unknown), after `pregap` of pregap (zero without one).
    pub fn new(duration: Option<Duration>, pregap: Duration) -> Self {
        Self { pregap, duration }
    }

    /// How long the track runs from INDEX 01, when known.
    pub fn duration(self) -> Option<Duration> {
        self.duration
    }

    pub fn duration_ms(self) -> Option<u64> {
        self.duration.map(|duration| duration.as_millis() as u64)
    }

    /// How long the track's stream runs, its pregap and then the track, when
    /// the track's length is known.
    pub fn stream_duration(self) -> Option<Duration> {
        self.duration.map(|duration| self.pregap + duration)
    }

    /// Where the track starts in its stream: INDEX 01, past the pregap.
    pub fn track_start(self) -> StreamPosition {
        StreamPosition(self.pregap)
    }

    /// The track time at `position`: negative in the pregap, zero at INDEX 01,
    /// and held at a known duration if decoded audio runs past the declared
    /// end. Without a known duration it runs on with the audio.
    pub fn track_time(self, position: StreamPosition) -> TrackTime {
        let ms = i128::from(position.as_millis()) - self.pregap.as_millis() as i128;
        let ms = match self.duration_ms() {
            Some(duration_ms) => ms.min(i128::from(duration_ms)),
            None => ms,
        };
        TrackTime(i64::try_from(ms).expect("playback position exceeds i64 milliseconds"))
    }

    /// The stream position at track time `time`, held at the stream's start.
    /// No upper bound: a position past the end plays nothing and completes.
    pub fn stream_position(self, time: TrackTime) -> StreamPosition {
        let ms = i128::from(time.0) + self.pregap.as_millis() as i128;
        StreamPosition::from_millis(u64::try_from(ms.max(0)).unwrap_or(u64::MAX))
    }

    /// The seek bar's fill (0.0–1.0) at `position`: zero through the pregap,
    /// one at the track's end. Zero throughout when the length is unknown or
    /// zero, since there is no end to fill toward.
    pub fn progress(self, position: StreamPosition) -> f64 {
        let Some(duration_ms) = self.duration_ms().filter(|ms| *ms > 0) else {
            return 0.0;
        };
        let time = self.track_time(position).0;
        (time.max(0) as f64 / duration_ms as f64).clamp(0.0, 1.0)
    }

    /// The stream position a seek bar fill points at — the inverse of
    /// [`Self::progress`]. `None` when the length is unknown: a fill is a
    /// fraction of a length, so without one it points nowhere.
    pub fn position_at_progress(self, ratio: f64) -> Option<StreamPosition> {
        let duration_ms = self.duration_ms()?;
        let into_track = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
        Some(StreamPosition::from_millis(
            self.pregap.as_millis() as u64 + into_track,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline(duration_ms: u64, pregap_ms: u64) -> TrackTimeline {
        TrackTimeline::new(
            Some(Duration::from_millis(duration_ms)),
            Duration::from_millis(pregap_ms),
        )
    }

    /// The timeline of a track whose length nothing knows.
    fn unknown_length(pregap_ms: u64) -> TrackTimeline {
        TrackTimeline::new(None, Duration::from_millis(pregap_ms))
    }

    fn at(ms: u64) -> StreamPosition {
        StreamPosition::from_millis(ms)
    }

    // -- progress --
    //
    // Progress is 0.0-1.0 representing position within the track (after pregap).
    // During pregap, progress stays at 0 -- the slider doesn't move until the
    // track starts. At the end of the track, progress reaches 1.0.

    #[test]
    fn progress_no_pregap() {
        let t = timeline(10_000, 0);
        assert_eq!(t.progress(at(0)), 0.0);
        assert_eq!(t.progress(at(5_000)), 0.5);
        assert_eq!(t.progress(at(10_000)), 1.0);
    }

    #[test]
    fn progress_stays_zero_during_pregap() {
        let t = timeline(10_000, 2000);
        assert_eq!(t.progress(at(0)), 0.0);
        assert_eq!(t.progress(at(1000)), 0.0);
        assert_eq!(t.progress(at(1999)), 0.0);
    }

    #[test]
    fn progress_starts_after_pregap() {
        // 10s track plus a 2s pregap. At 7s in the stream = 5s into track = 50%.
        let t = timeline(10_000, 2000);
        assert_eq!(t.progress(at(2000)), 0.0);
        assert_eq!(t.progress(at(7000)), 0.5);
        assert_eq!(t.progress(at(12_000)), 1.0);
    }

    #[test]
    fn progress_zero_duration() {
        assert_eq!(timeline(0, 0).progress(at(0)), 0.0);
    }

    /// Without a length there is no end to fill toward, so the bar stays empty
    /// however far the track plays.
    #[test]
    fn progress_stays_empty_without_a_length() {
        let t = unknown_length(2000);
        assert_eq!(t.progress(at(0)), 0.0);
        assert_eq!(t.progress(at(400_000)), 0.0);
    }

    /// An unknown length is no length: not the track's, and not its stream's.
    #[test]
    fn an_unknown_length_has_no_duration() {
        let t = unknown_length(2000);
        assert_eq!(t.duration(), None);
        assert_eq!(t.duration_ms(), None);
        assert_eq!(t.stream_duration(), None);
        assert_eq!(t.track_start(), at(2000));
    }

    // -- track_time --
    //
    // The stream includes the pregap; the stored duration does not. The player
    // shows a negative countdown before INDEX 01, zero at the track's start, and
    // the stored duration unchanged.

    #[test]
    fn track_time_no_pregap() {
        assert_eq!(
            timeline(60_000, 0).track_time(at(5000)),
            TrackTime::from_millis(5000)
        );
    }

    #[test]
    fn track_time_counts_down_during_pregap() {
        let t = timeline(10_000, 2000);
        assert_eq!(t.track_time(at(0)), TrackTime::from_millis(-2000));
        assert_eq!(t.track_time(at(1000)), TrackTime::from_millis(-1000));
        assert_eq!(t.track_time(at(1999)), TrackTime::from_millis(-1));
        assert_eq!(t.duration_ms(), Some(10_000));
    }

    #[test]
    fn track_time_subtracts_the_pregap() {
        assert_eq!(
            timeline(10_000, 2000).track_time(at(5000)),
            TrackTime::from_millis(3000)
        );
    }

    #[test]
    fn track_time_holds_at_the_duration_past_the_end() {
        assert_eq!(
            timeline(10_000, 2000).track_time(at(13_000)),
            TrackTime::from_millis(10_000)
        );
    }

    /// With no declared end there is nothing to hold the time at: it runs on
    /// for as long as the audio does.
    #[test]
    fn track_time_runs_on_without_a_length() {
        let t = unknown_length(2000);
        assert_eq!(t.track_time(at(0)), TrackTime::from_millis(-2000));
        assert_eq!(t.track_time(at(402_000)), TrackTime::from_millis(400_000));
    }

    // -- stream_position --

    #[test]
    fn stream_position_inverts_track_time() {
        let t = timeline(10_000, 2000);
        for ms in [-2000, -500, 0, 3000, 10_000] {
            let time = TrackTime::from_millis(ms);
            assert_eq!(t.track_time(t.stream_position(time)), time, "{ms}ms");
        }
        assert_eq!(t.stream_position(TrackTime::START), t.track_start());
    }

    #[test]
    fn stream_position_holds_at_the_stream_start() {
        assert_eq!(
            timeline(10_000, 2000).stream_position(TrackTime::from_millis(-5000)),
            StreamPosition::START
        );
    }

    // -- position_at_progress --
    //
    // The inverse of progress. A slider drawn from one and dragged back through
    // the other must land where the user dropped it, whatever the pregap.

    #[test]
    fn position_at_progress_inverts_progress() {
        for pregap in [0, 2000] {
            let t = timeline(10_000, pregap);
            for ratio in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let position = t
                    .position_at_progress(ratio)
                    .expect("a timeline with a length has a position at every fill");
                let round_tripped = t.progress(position);
                assert!(
                    (round_tripped - ratio).abs() < 1e-9,
                    "pregap {pregap:?} ratio {ratio} -> {position:?} -> {round_tripped}"
                );
            }
        }
    }

    #[test]
    fn position_at_progress_offsets_past_the_pregap() {
        // 10s track plus a 2s pregap. Halfway is 5s in, i.e. 7s into the stream.
        let t = timeline(10_000, 2000);
        assert_eq!(t.position_at_progress(0.0), Some(at(2000)));
        assert_eq!(t.position_at_progress(0.5), Some(at(7000)));
        assert_eq!(t.position_at_progress(1.0), Some(at(12_000)));
    }

    /// A fill is a fraction of the length; without one it points nowhere.
    #[test]
    fn position_at_progress_has_no_position_without_a_length() {
        let t = unknown_length(2000);
        for ratio in [0.0, 0.5, 1.0] {
            assert_eq!(t.position_at_progress(ratio), None, "ratio {ratio}");
        }
    }
}
