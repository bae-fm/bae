//! Resolved album types.
//!
//! Only `AlbumSummary`'s projection is pure enough to own its constructor here.
//! `AlbumDetail`'s needs covers and whether storage is set up, so the album
//! view resolves it (`crate`).

use super::*;
use crate::DbAlbumAppearance;
use bae_records::{DbAlbum, DbAlbumSummary};

/// An album with its releases, each carrying tracks, files, and gallery.
#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: DbAlbum,
    /// The album's credits in list order, each the artist it shows after
    /// merges.
    pub artists: Vec<bae_records::credits::LibraryCredit>,
    /// The album's credit line: each credit's printed name, then the join its
    /// source prints after it (the plain join where it states none).
    pub artist_line: bae_types::credits::CreditLine,
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
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_line: bae_types::credits::CreditLine,
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
            artist_line: raw.artist_line,
            release_ids: raw.release_ids,
            primary_release_id,
            cover,
        }
    }
}

/// One row of the album grid: an album, listed once when the grid is flat and
/// under each artist its credits show when grouped by artist.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumAppearance {
    pub album: AlbumSummary,
    /// Grouped by artist: the album's credits other than the artist whose
    /// group lists it, printed as a line; `None` when that artist is its only
    /// credit, or when the grid is flat.
    pub credited_with: Option<bae_types::credits::CreditLine>,
}

impl AlbumAppearance {
    pub(crate) fn from_raw(raw: DbAlbumAppearance) -> AlbumAppearance {
        AlbumAppearance {
            album: AlbumSummary::from_raw(raw.album),
            credited_with: raw.credited_with,
        }
    }
}
