//! A position within one track, as the player shows it.

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
