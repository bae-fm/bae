//! Where playback is, for the surfaces that move the position on themselves:
//! the operating system's media controls and Media3. Each is told a line, a
//! position and whether it moves from there, and draws the rest of it; it is
//! told again only when playback leaves the line it holds: a seek, another
//! track or clip, a pause or resume, a new length, or a position that lands
//! off where the line puts it. Views that draw the position itself read the
//! positions as they move instead.

use chrono::{DateTime, Utc};

use super::{PlaybackPositions, PlaybackValues, PreviewState};
use crate::playback::service::PlaybackState;
use crate::playback::PreviewTarget;

/// How far a position may land from where its line puts it before the line
/// is told again: past the jitter of position ticks, which arrive a few times
/// a second, and short of any jump a listener would notice.
pub const JUMP_THRESHOLD_MS: u64 = 1_000;

/// The library track's line and the preview's, as last told.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackTimeline {
    /// The library track's line, on the timeline operating-system media
    /// controls accept: no pregap. `None` until the playing track reports a
    /// position, and again once playback moves to another track or stops.
    pub library: Option<TimelinePoint>,
    /// The applied seeks so far; it moves with each one, so a surface that
    /// projected a seek knows when core applied it.
    pub seek_revision: u64,
    /// `None` while no preview is loaded.
    pub preview: Option<PreviewTimelinePoint>,
}

/// One track's line: where it was when told, and whether it moves on.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelinePoint {
    pub track_id: String,
    pub position_ms: u64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    /// The track plays past its pregap, so the position moves on at the
    /// play rate. Through the pregap it stands at the track's start, which
    /// is as early as this timeline goes.
    pub moving: bool,
}

/// The preview clip's line.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewTimelinePoint {
    pub target: PreviewTarget,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub moving: bool,
}

impl PlaybackTimeline {
    pub(super) fn initial() -> Self {
        Self {
            library: None,
            seek_revision: 0,
            preview: None,
        }
    }
}

/// A line's identity and how fast it moves: a change to any of these is a
/// new line whatever the position.
trait Line {
    fn position_ms(&self) -> u64;
    fn same_line(&self, other: &Self) -> bool;
    fn moving(&self) -> bool;
}

impl Line for TimelinePoint {
    fn position_ms(&self) -> u64 {
        self.position_ms
    }

    fn same_line(&self, other: &Self) -> bool {
        self.track_id == other.track_id
            && self.duration_ms == other.duration_ms
            && self.moving == other.moving
    }

    fn moving(&self) -> bool {
        self.moving
    }
}

impl Line for PreviewTimelinePoint {
    fn position_ms(&self) -> u64 {
        self.position_ms
    }

    fn same_line(&self, other: &Self) -> bool {
        self.target == other.target
            && self.duration_ms == other.duration_ms
            && self.moving == other.moving
    }

    fn moving(&self) -> bool {
        self.moving
    }
}

/// One line as told, and when.
#[derive(Debug, Clone)]
struct Told<Point> {
    point: Point,
    at: DateTime<Utc>,
}

impl<Point: Line + Clone> Told<Point> {
    /// Whether `next`, read at `now`, leaves the line this was told.
    fn leaves(&self, next: &Point, now: DateTime<Utc>) -> bool {
        if !self.point.same_line(next) {
            return true;
        }
        let elapsed_ms = if self.point.moving() {
            (now - self.at).num_milliseconds().max(0) as u64
        } else {
            0
        };
        let expected = self.point.position_ms().saturating_add(elapsed_ms);
        expected.abs_diff(next.position_ms()) > JUMP_THRESHOLD_MS
    }
}

/// The line each surface holds now, as last told.
fn fold_point<Point: Line + Clone>(
    told: &mut Option<Told<Point>>,
    next: Option<Point>,
    now: DateTime<Utc>,
) -> bool {
    let leaves = match (told.as_ref(), next.as_ref()) {
        (None, None) => false,
        (Some(told), Some(next)) => told.leaves(next, now),
        (None, Some(_)) | (Some(_), None) => true,
    };
    if leaves {
        *told = next.map(|point| Told { point, at: now });
    }
    leaves
}

/// The timeline the positions and values fold to, told again only as a line
/// changes.
pub(super) struct TimelineFold {
    library: Option<Told<TimelinePoint>>,
    seek_revision: u64,
    preview: Option<Told<PreviewTimelinePoint>>,
}

impl TimelineFold {
    pub(super) fn new() -> Self {
        Self {
            library: None,
            seek_revision: 0,
            preview: None,
        }
    }

    /// The timeline after the positions and values read at `now`, or `None`
    /// when neither line changed.
    pub(super) fn next(
        &mut self,
        positions: &PlaybackPositions,
        values: &PlaybackValues,
        now: DateTime<Utc>,
    ) -> Option<PlaybackTimeline> {
        let library = positions.library.as_ref().map(|position| {
            // The pregap plays before the track's start, where this timeline
            // has nowhere to put it: the line stands there until it ends.
            let moving = position.position_ms >= 0
                && matches!(
                    &values.state,
                    PlaybackState::Playing { track } if track.track_id == position.track_id
                );
            TimelinePoint {
                track_id: position.track_id.clone(),
                position_ms: position.position_ms.max(0) as u64,
                duration_ms: position.duration_ms,
                moving,
            }
        });
        let preview = positions
            .preview
            .as_ref()
            .map(|preview| PreviewTimelinePoint {
                target: preview.target.clone(),
                position_ms: preview.position_ms,
                duration_ms: preview.duration_ms,
                moving: matches!(
                    &values.preview,
                    PreviewState::Playing { target, .. } if *target == preview.target
                ),
            });
        let seeked = positions.seek_revision != self.seek_revision;
        self.seek_revision = positions.seek_revision;
        // A seek is a new line even when it lands where the line already was.
        if seeked {
            self.library = None;
        }
        let library_changed = fold_point(&mut self.library, library, now);
        let preview_changed = fold_point(&mut self.preview, preview, now);
        (library_changed || preview_changed || seeked).then(|| PlaybackTimeline {
            library: self.library.as_ref().map(|told| told.point.clone()),
            seek_revision: self.seek_revision,
            preview: self.preview.as_ref().map(|told| told.point.clone()),
        })
    }
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
