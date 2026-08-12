//! Resolved album types.
//!
//! Only `AlbumSummary`'s projection is pure enough to own its constructor here.
//! `AlbumDetail`'s needs covers and per-release pin state, so it lives on
//! `LibraryManager`.

use super::*;
use crate::db::{DbAlbum, DbAlbumSummary};

/// An album with its releases, each carrying tracks, files, and gallery.
#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: DbAlbum,
    /// Comma-joined artist names for display.
    pub artist_names: String,
    pub releases: Vec<ReleaseDetail>,
    /// The user's chosen primary release, else the first. Always set: every album has
    /// at least one release, enforced by `delete_release`, which removes the album row
    /// when its last release goes.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. The version moves when the bytes do, so subscribers receive a changed
    /// field and the UI re-renders.
    pub cover: Option<ImageRef>,
}

/// The slim projection a list view renders, with the `primary_release_id` fallback
/// already applied. Composed into [`AlbumDetail`] for detail views.
#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    pub release_ids: Vec<String>,
    /// The user's chosen primary release, else the first. Always set: every album has
    /// at least one release, enforced by `delete_release`.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. Carried on the *summary* so a cover change updates its subscribed
    /// value: the version moves when the bytes do and the cover reloads.
    pub cover: Option<ImageRef>,
}

impl AlbumSummary {
    /// Applies the `primary_release_id` fallback through
    /// [`crate::db::resolve_primary_release_id`], the one definition of it. It
    /// always succeeds: every album has at least one release.
    ///
    /// `resolve_cover` maps that primary release id to its cover reference. It is
    /// passed in rather than reached for through `&self`, so one resolver serves the
    /// SQL-page, search, and event paths without each duplicating the
    /// existence-and-version logic.
    pub(crate) fn from_raw(
        raw: DbAlbumSummary,
        resolve_cover: impl Fn(&str) -> Option<ImageRef>,
    ) -> AlbumSummary {
        let primary_release_id = crate::db::resolve_primary_release_id(
            raw.primary_release_id.as_deref(),
            raw.release_ids.iter().map(String::as_str),
        )
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
