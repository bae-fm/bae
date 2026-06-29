//! Resolved album types (`AlbumSummary`, `AlbumDetail`) and the pure projection
//! that produces `AlbumSummary` from its raw aggregate.
//!
//! `AlbumDetail`'s constructor reads `&self` (covers, pin state per release) and
//! lives on `LibraryManager`; only the slim `AlbumSummary` projection is pure
//! enough to own its constructor here.

use super::*;
use crate::db::{DbAlbum, DbAlbumSummary};

/// Full album detail: album + releases (with tracks, files, gallery).
#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: DbAlbum,
    /// Comma-joined artist names for display.
    pub artist_names: String,
    pub releases: Vec<ReleaseDetail>,
    /// User-chosen primary release, falling back to first. Always set:
    /// every album has at least one release (enforced by `delete_release`
    /// which removes the album row when its last release is deleted).
    pub primary_release_id: String,
    /// Reference to the album's cover — the primary release's cover (image id +
    /// version) — or `None` when it has no cover row. The version moves when the
    /// cover bytes do, so the `AlbumUpdated` payload carries a changed field and
    /// the UI re-renders the cover.
    pub cover: Option<ImageRef>,
}

/// Resolved album summary: the slim projection list views render. Produced
/// by `LibraryManager` from `DbAlbumSummary`; carries the
/// `primary_release_id` fallback applied.
///
/// Composed into [`AlbumDetail`] for detail views. Interned into the
/// UI-side "summaries" slice alongside the summary/detail pattern
/// described at the top of this file.
#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    pub release_ids: Vec<String>,
    /// User-chosen primary release, or the first release if unset. Always
    /// set: every album has at least one release (enforced by
    /// `delete_release` which removes the album row when its last release
    /// is deleted).
    pub primary_release_id: String,
    /// Reference to the album's cover — the primary release's cover (image id +
    /// version) — or `None` when it has no cover row. Carried on the summary so a
    /// cover change produces a changed field on the `AlbumUpdated` event: the
    /// version moves when the bytes do, so the UI's per-field re-render fires and
    /// the cover reloads.
    pub cover: Option<ImageRef>,
}

impl AlbumSummary {
    /// Apply the `primary_release_id` fallback: stored value, or the first release
    /// if unset. Produces a resolved `AlbumSummary` from a raw `DbAlbumSummary`,
    /// with the album's cover resolved to its cache-bustable identifier via
    /// `resolve_cover`. The fallback always succeeds: every album has at least one
    /// release (enforced by `delete_release`).
    ///
    /// `resolve_cover` maps the primary release id to its cover reference (image id +
    /// version) from the `covers` row; passed in rather than reaching for `&self` so
    /// the same resolver serves the SQL-page, search, and event paths without
    /// duplicating the existence-and-version logic.
    pub(crate) fn from_raw(
        raw: DbAlbumSummary,
        resolve_cover: impl Fn(&str) -> Option<ImageRef>,
    ) -> AlbumSummary {
        let primary_release_id = raw
            .primary_release_id
            .clone()
            .or_else(|| raw.release_ids.first().cloned())
            .expect("album has at least one release");
        let cover = resolve_cover(&primary_release_id);
        AlbumSummary {
            id: raw.id,
            title: raw.title,
            year: raw.year,
            is_compilation: raw.is_compilation,
            artist_names: raw.artist_names,
            release_ids: raw.release_ids,
            primary_release_id,
            cover,
        }
    }
}
