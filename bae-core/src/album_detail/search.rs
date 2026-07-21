//! Resolved search-result types (`SearchResults`, `AlbumSearchResult`,
//! `TrackSearchResult`, composer hits, and work hits) and the pure projections
//! that produce them.

use std::collections::HashMap;

use super::*;
use crate::db::DbLibrarySearchResults;

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub albums: Vec<AlbumSearchResult>,
    pub tracks: Vec<TrackSearchResult>,
    pub composers: Vec<ComposerSummary>,
    pub works: Vec<WorkSummary>,
}

impl SearchResults {
    /// `release_covers` is keyed by release id and serves every release-cover
    /// lookup the raw rows need: album primary release ids, track release ids,
    /// and work representative release ids. `composer_images` is keyed by
    /// artist id.
    pub(crate) fn from_raw(
        raw: DbLibrarySearchResults,
        release_covers: &HashMap<String, ImageRef>,
        composer_images: &HashMap<String, ImageRef>,
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
                    let cover = release_covers.get(&primary_release_id).cloned();
                    AlbumSearchResult {
                        id: raw.id,
                        title: raw.title,
                        year: raw.year,
                        artist_name: raw.artist_name,
                        cover,
                    }
                })
                .collect(),
            tracks: raw
                .tracks
                .into_iter()
                .map(|raw| {
                    let cover = release_covers.get(&raw.release_id).cloned();
                    TrackSearchResult {
                        id: raw.id,
                        title: raw.title,
                        duration_ms: raw.duration_ms,
                        album_id: raw.album_id,
                        album_title: raw.album_title,
                        artist_name: raw.artist_name,
                        cover,
                    }
                })
                .collect(),
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
                        .and_then(|id| release_covers.get(id).cloned());
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

/// A resolved track hit: the raw row's display fields plus the cover of the
/// release the track belongs to.
#[derive(Debug, Clone)]
pub struct TrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    /// The cover of the track's own release, or `None` when there is no cover
    /// row. Same fetch/caching contract as [`AlbumSearchResult::cover`].
    pub cover: Option<ImageRef>,
}
