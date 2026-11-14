//! Resolved search-result types (`SearchResults`, `AlbumSearchResult`,
//! `TrackSearchResult`, composer hits, and work hits) and the pure projections
//! that produce them.

use super::*;
use crate::{
    DbAlbumSearchResult, DbArtistSummary, DbComposerSummary, DbLibrarySearchResults, DbTopPlayable,
    DbTrackSearchResult, DbWorkSummary,
};

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub albums: Vec<AlbumSearchResult>,
    pub artists: Vec<DbArtistSummary>,
    pub tracks: Vec<TrackSearchResult>,
    pub composers: Vec<DbComposerSummary>,
    pub works: Vec<DbWorkSummary>,
    /// The track a spoken "play" of the search plays: the first track found,
    /// else the first track of the first album found.
    pub top_playable: Option<DbTopPlayable>,
    /// Whether the search found anything of any kind.
    pub found: bool,
}

impl SearchResults {
    /// Each hit as its row reads it, with the version of its cover or image.
    pub(crate) fn from_raw(raw: DbLibrarySearchResults) -> SearchResults {
        SearchResults {
            albums: raw.albums.into_iter().map(AlbumSearchResult::from_raw).collect(),
            tracks: raw.tracks.into_iter().map(TrackSearchResult::from_raw).collect(),
            artists: raw.artists,
            composers: raw.composers,
            works: raw.works,
            top_playable: raw.top_playable,
            found: raw.found,
        }
    }
}

/// A field-by-field copy of `DbAlbumSearchResult` under the public name.
#[derive(Debug, Clone)]
pub struct AlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_line: bae_types::credits::CreditLine,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. The search UI fetches the bytes by image id and caches under
    /// `(id, version)`. The primary release id itself isn't surfaced: search
    /// navigates by album id.
    pub cover: Option<ImageRef>,
}

/// A resolved track hit: the raw row's display fields plus the cover of the
/// release the track belongs to.
#[derive(Debug, Clone)]
pub struct TrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    /// The release the track is on — what playing it from a search starts.
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_line: bae_types::credits::CreditLine,
    /// The cover of the track's own release, or `None` when there is no cover
    /// row. Same fetch/caching contract as [`AlbumSearchResult::cover`].
    pub cover: Option<ImageRef>,
}

impl AlbumSearchResult {
    pub(crate) fn from_raw(raw: DbAlbumSearchResult) -> AlbumSearchResult {
        let cover = cover_ref(&raw.primary_release_id, raw.cover_version);
        AlbumSearchResult {
            id: raw.id,
            title: raw.title,
            year: raw.year,
            artist_line: raw.artist_line,
            cover,
        }
    }
}

impl TrackSearchResult {
    pub(crate) fn from_raw(raw: DbTrackSearchResult) -> TrackSearchResult {
        let cover = cover_ref(&raw.release_id, raw.cover_version);
        TrackSearchResult {
            id: raw.id,
            title: raw.title,
            duration_ms: raw.duration_ms,
            release_id: raw.release_id,
            album_id: raw.album_id,
            album_title: raw.album_title,
            artist_line: raw.artist_line,
            cover,
        }
    }
}
