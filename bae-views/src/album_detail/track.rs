//! A release's tracks as its detail shows them, grouped by the side of the
//! record each sits on.

use bae_types::mediums::{TrackPosition, TrackSide};
use bae_types::stars::Stars;

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
    /// The works the track performs, in the order it performs them.
    pub works: Vec<TrackWork>,
    /// How the track's row shows this person's rating of it under the
    /// library's rating system for tracks.
    pub rating: TrackRating,
}

/// A track's rating as its row shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackRating {
    /// *None*: the row shows no rating.
    Hidden,
    /// *Stars*: the stars this person gave the track.
    Stars(Option<Stars>),
    /// *Heart*: whether this person hearted the track.
    Heart(bool),
}

impl TrackRating {
    fn of(system: bae_people::people::RatingSystem, stars: Option<Stars>) -> Self {
        use bae_people::people::{is_heart, RatingSystem};
        match system {
            RatingSystem::Hidden => Self::Hidden,
            RatingSystem::Stars => Self::Stars(stars),
            RatingSystem::Heart => Self::Heart(stars.is_some_and(is_heart)),
        }
    }
}

/// A work a track performs.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackWork {
    pub work_id: String,
    pub title: String,
    /// The work's composers printed in credit order, as a work's summary
    /// prints them; `None` when the work has none.
    pub composer_names: Option<String>,
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
    /// Whether the release has more than one group, so each shows its own
    /// time beside the release's.
    pub shows_time: bool,
    /// The place among the group's tracks of the first one its second column
    /// lists, or `None` when one column lists them all.
    pub second_column_start: Option<u32>,
}

impl TrackGroup {
    /// The group `raw` holds, whose tracks show an artist each when the
    /// album is a compilation.
    pub(crate) fn from_raw(
        raw: bae_records::DbTrackGroup,
        is_compilation: bool,
        rating_system: bae_people::people::RatingSystem,
    ) -> Self {
        TrackGroup {
            side: raw.side,
            tracks: raw
                .tracks
                .into_iter()
                .map(|track| TrackDetail::from_raw(track, is_compilation, rating_system))
                .collect(),
            total_duration_ms: raw.total_duration_ms,
            shows_time: raw.shows_time,
            second_column_start: raw.second_column_start,
        }
    }
}

impl TrackDetail {
    /// The track `raw` holds. On a compilation each row carries its own
    /// artist, because the album header names no single one; on a
    /// single-artist album the row would only repeat the header, so it shows
    /// none.
    fn from_raw(
        raw: bae_records::DbTrackWithArtists,
        is_compilation: bool,
        rating_system: bae_people::people::RatingSystem,
    ) -> Self {
        let artist_line = bae_records::credits::library_line(&raw.shown_artists);
        TrackDetail {
            display_artist: is_compilation.then(|| artist_line.clone()),
            artist_line,
            position_text: raw.position.text(),
            position: raw.position,
            works: raw
                .works
                .into_iter()
                .map(|work| TrackWork {
                    work_id: work.work_id,
                    title: work.title,
                    composer_names: work.composer_names,
                })
                .collect(),
            rating: TrackRating::of(rating_system, raw.my_stars),
            id: raw.id,
            title: raw.title,
            duration_ms: raw.duration_ms,
        }
    }
}
