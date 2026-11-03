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

use bae_types::track_time::TrackTime;
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
        TrackTime::from_millis(
            i64::try_from(ms).expect("playback position exceeds i64 milliseconds"),
        )
    }

    /// The stream position at track time `time`, held at the stream's start.
    /// No upper bound: a position past the end plays nothing and completes.
    pub fn stream_position(self, time: TrackTime) -> StreamPosition {
        let ms = i128::from(time.as_millis()) + self.pregap.as_millis() as i128;
        StreamPosition::from_millis(u64::try_from(ms.max(0)).unwrap_or(u64::MAX))
    }

    /// The seek bar's fill (0.0–1.0) at `position`: zero through the pregap,
    /// one at the track's end. Zero throughout when the length is unknown or
    /// zero, since there is no end to fill toward.
    pub fn progress(self, position: StreamPosition) -> f64 {
        let Some(duration_ms) = self.duration_ms().filter(|ms| *ms > 0) else {
            return 0.0;
        };
        let time = self.track_time(position).as_millis();
        (time.max(0) as f64 / duration_ms as f64).clamp(0.0, 1.0)
    }

    /// The stream position a seek bar fill points at — the inverse of
    /// [`Self::progress`]. `None` when the length is unknown: a fill is a
    /// fraction of a length, so without one it points nowhere.
    pub fn position_at_progress(self, ratio: f64) -> Option<StreamPosition> {
        let duration_ms = self.duration_ms()?;
        let into_track = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
        Some(StreamPosition::from_millis(self.pregap.as_millis() as u64 + into_track))
    }
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
