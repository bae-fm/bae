//! Resolved search-result types (`SearchResults`, `AlbumSearchResult`,
//! `TrackSearchResult`, composer hits, and work hits) and the pure projections
//! that produce them.

use std::collections::HashMap;

use super::*;
use crate::db::{DbLibrarySearchResults, DbTrackSearchResult};

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub albums: Vec<AlbumSearchResult>,
    pub tracks: Vec<TrackSearchResult>,
    pub composers: Vec<ComposerSummary>,
    pub works: Vec<WorkSummary>,
}

impl SearchResults {
    /// The cover/image maps are keyed by what the raw search rows carry: album
    /// primary release ids, composer artist ids, and work representative release ids.
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
                    // Always resolves: every album has at least one release
                    // (enforced by `delete_release`).
                    let primary_release_id = crate::db::resolve_primary_release_id(
                        raw.primary_release_id.as_deref(),
                        raw.release_ids.iter().map(String::as_str),
                    )
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

/// A field-by-field copy of `DbAlbumSearchResult` under the public name.
#[derive(Debug, Clone)]
pub struct AlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. The search UI fetches the bytes by image id and caches under
    /// `(id, version)`. The primary release id itself isn't surfaced: search
    /// navigates by album id.
    pub cover: Option<ImageRef>,
}

pub type TrackSearchResult = DbTrackSearchResult;
