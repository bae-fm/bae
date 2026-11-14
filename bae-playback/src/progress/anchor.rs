//! Where the playing track is, as the position stream holds it: an anchor, a
//! position at a moment and the rate it moves at from there, which the native
//! view draws its position from each display frame (position, plus the time
//! since the anchor times the rate). The position ticks core gets a few times
//! a second never reach the screen; the stream is told again only as playback
//! leaves the line it holds: a seek, a pause or resume, a change of rate,
//! another track or length, or a position that lands off where the line puts
//! it.

use super::timeline::JUMP_THRESHOLD_MS;
use super::PlaybackPositions;

/// The playing track's line: where it was at `at_ns`, and how fast it moves
/// on.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackAnchor {
    pub track_id: String,
    /// Track-relative at the anchor: negative while pregap audio plays.
    pub position_ms: i64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    /// How fast the position moves on from the anchor, as a multiple of real
    /// time: zero while the track is paused, loading or stopped, one while it
    /// plays here, a remote renderer's stated rate while it plays there.
    pub rate: f64,
    /// The applied seeks so far, so a bar that projected a seek knows when
    /// core applied it.
    pub seek_revision: u64,
    /// When the position was read, in nanoseconds on the frame clock
    /// (`bae_util::frame_clock::frame_clock_ns`).
    pub at_ns: u64,
}

impl TrackAnchor {
    /// Whether `next`, read later, lies along this line: the same track,
    /// length, rate and applied seeks, at the position the line puts it, give
    /// or take the jitter of position ticks.
    fn holds(&self, next: &Self) -> bool {
        let elapsed_ms = next.at_ns.saturating_sub(self.at_ns) as f64 / 1e6;
        let expected = self.position_ms as f64 + self.rate * elapsed_ms;
        self.track_id == next.track_id
            && self.duration_ms == next.duration_ms
            && self.rate == next.rate
            && self.seek_revision == next.seek_revision
            && (expected - next.position_ms as f64).abs() <= JUMP_THRESHOLD_MS as f64
    }
}

/// The anchor the positions fold to, told again only as the line changes.
#[derive(Debug, Default)]
pub struct AnchorFold {
    told: Option<TrackAnchor>,
}

impl AnchorFold {
    pub fn new() -> Self {
        Self::default()
    }

    /// The line the track is on, as last told: `None` until the playing track
    /// reports a position, and again once playback moves to another track or
    /// stops.
    pub fn anchor(&self) -> Option<&TrackAnchor> {
        self.told.as_ref()
    }

    /// Fold the positions read at `at_ns` on the frame clock; whether the
    /// line changed, in which case [`Self::anchor`] holds the new one.
    pub fn fold(&mut self, positions: &PlaybackPositions, at_ns: u64) -> bool {
        let next = positions.library.as_ref().map(|position| TrackAnchor {
            track_id: position.track_id.clone(),
            position_ms: position.position_ms,
            duration_ms: position.duration_ms,
            rate: if positions.playing { position.rate } else { 0.0 },
            seek_revision: positions.seek_revision,
            at_ns,
        });
        let holds = match (&self.told, &next) {
            (None, None) => true,
            (Some(told), Some(next)) => told.holds(next),
            (None, Some(_)) | (Some(_), None) => false,
        };
        if !holds {
            self.told = next;
        }
        !holds
    }
}

#[cfg(test)]
#[path = "anchor_tests.rs"]
mod tests;
