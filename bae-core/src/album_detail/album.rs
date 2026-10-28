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
    /// The album's credits in list order, each the artist it shows after
    /// merges.
    pub artists: Vec<crate::credits::LibraryCredit>,
    /// Comma-joined artist names for display.
    pub artist_names: String,
    pub releases: Vec<ReleaseDetail>,
    /// The album's lowest-ordered release: the one whose cover it shows and
    /// that plays by default.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. The version moves when the bytes do, so subscribers receive a changed
    /// field and the UI re-renders.
    pub cover: Option<ImageRef>,
}

/// The slim projection a list view renders. Composed into [`AlbumDetail`] for
/// detail views.
#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    /// The album's releases in album order.
    pub release_ids: Vec<String>,
    /// The album's lowest-ordered release: the one whose cover it shows and
    /// that plays by default.
    pub primary_release_id: String,
    /// The album's cover — its primary release's — or `None` when there is no cover
    /// row. Carried on the *summary* so a cover change updates its subscribed
    /// value: the version moves when the bytes do and the cover reloads.
    pub cover: Option<ImageRef>,
}

impl AlbumSummary {
    /// The album as its row reads it, its cover the one its primary release's
    /// cover version names.
    pub(crate) fn from_raw(raw: DbAlbumSummary) -> AlbumSummary {
        let primary_release_id = raw.primary_release_id;
        let cover = cover_ref(&primary_release_id, raw.cover_version);
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
