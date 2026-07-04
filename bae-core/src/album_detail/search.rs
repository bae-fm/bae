//! Resolved search-result types (`SearchResults`, `AlbumSearchResult`,
//! `TrackSearchResult`, composer hits, and work hits) and the pure projections
//! that produce them.

use std::collections::HashMap;

use super::*;
use crate::db::{DbLibrarySearchResults, DbTrackSearchResult};

/// Resolved search-result container produced by `LibraryManager` from
/// `DbLibrarySearchResults`.
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub albums: Vec<AlbumSearchResult>,
    pub tracks: Vec<TrackSearchResult>,
    pub composers: Vec<ComposerSummary>,
    pub works: Vec<WorkSummary>,
}

impl SearchResults {
    /// Resolve the raw search-result container by mapping each inner list.
    /// Cover/image maps are keyed by the ids carried by the raw search rows:
    /// album primary release ids, composer artist ids, and work representative
    /// release ids.
    pub(crate) fn from_raw(
        raw: DbLibrarySearchResults,
        album_covers: &HashMap<String, ImageRef>,
        composer_images: &HashMap<String, ImageRef>,
        work_covers: &HashMap<String, ImageRef>,
    ) -> SearchResults {
        SearchResults {
            albums: raw
                .albums
                .into_iter()
                .map(|raw| {
                    // The primary release id resolves the cover but isn't surfaced —
                    // the search UI navigates by album id. The raw `primary_release_id`
                    // comes from SQL's `COALESCE(a.primary_release_id, <first release
                    // id>)` and is non-null by construction: every album has at least
                    // one release (enforced by `delete_release`).
                    let primary_release_id = raw
                        .primary_release_id
                        .expect("album has at least one release");
                    let cover = album_covers.get(&primary_release_id).cloned();
                    AlbumSearchResult {
                        id: raw.id,
                        title: raw.title,
                        year: raw.year,
                        artist_name: raw.artist_name,
                        cover,
                    }
                })
                .collect(),
            tracks: raw.tracks,
            composers: raw
                .composers
                .into_iter()
                .map(|composer| {
                    let image = composer_images.get(&composer.artist.id).cloned();
                    ComposerSummary::from_raw(composer, image)
                })
                .collect(),
            works: raw
                .works
                .into_iter()
                .map(|work| {
                    let cover = work
                        .representative_release_id
                        .as_ref()
                        .and_then(|id| work_covers.get(id).cloned());
                    WorkSummary::from_raw(work, cover)
                })
                .collect(),
        }
    }
}

/// Resolved album search result. Field-by-field copy of
/// `DbAlbumSearchResult` — no transformation, just the "public" name.
#[derive(Debug, Clone)]
pub struct AlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    /// Reference to the album's cover — the primary release's cover (image id +
    /// version) — or `None` when it has no cover row. The search UI fetches its
    /// bytes by image id and caches under `(id, version)`. Resolved from the
    /// album's primary release; the primary id itself isn't surfaced (the search
    /// UI navigates by album id).
    pub cover: Option<ImageRef>,
}

pub type TrackSearchResult = DbTrackSearchResult;
