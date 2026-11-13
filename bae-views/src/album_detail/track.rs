//! A release's tracks as its detail shows them, grouped by the side of the
//! record each sits on.

use bae_types::mediums::{TrackPosition, TrackSide};

/// Track with resolved artist names and a structured display position.
#[derive(Debug, Clone)]
pub struct TrackDetail {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    /// Effective artist names for display -- the track's own artists when it
    /// has per-track artist rows, otherwise the album artists. Always
    /// populated so UI consumers can render a row label without joining
    /// artist data themselves.
    pub artist_line: bae_types::credits::CreditLine,
    /// The artist to show on the track row, or `None` when the row should show
    /// none. Set only for a compilation, whose header names no single artist so
    /// each row needs its own; on a single-artist album the row would only repeat
    /// the header. Whether a row shows an artist is a domain decision, made here.
    pub display_artist: Option<bae_types::credits::CreditLine>,
    /// Core-rendered position string: "A1"/"3"/"5", or the stable prefix
    /// when the source has no track number.
    pub position_text: String,
    /// Structured position retained for side/disc grouping.
    pub position: TrackPosition,
}

/// The tracks of one side. A single-side release has one `Flat` group.
#[derive(Debug, Clone)]
pub struct TrackGroup {
    pub side: TrackSide,
    pub tracks: Vec<TrackDetail>,
    /// Playing time of this group's tracks. A multi-disc release shows it
    /// under each disc, where the album-wide total alone reads as belonging to
    /// the last disc.
    pub total_duration_ms: u64,
}

/// Group pre-sorted tracks by consecutive side. Two tracks share a group when
/// their positions resolve to the same [`TrackSide`] (same side letter, same
/// disc, or both flat).
pub(crate) fn group_tracks_by_side(tracks: &[TrackDetail]) -> Vec<TrackGroup> {
    let mut groups: Vec<TrackGroup> = Vec::new();
    for track in tracks {
        let side = track.position.side();
        if let Some(last) = groups.last_mut() {
            if last.side == side {
                last.tracks.push(track.clone());
                last.total_duration_ms += track.duration_ms;
                continue;
            }
        }
        groups.push(TrackGroup {
            side,
            tracks: vec![track.clone()],
            total_duration_ms: track.duration_ms,
        });
    }
    groups
}

#[cfg(test)]
#[path = "track_tests.rs"]
mod tests;
