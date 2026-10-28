//! Resolved artist types and their pure projections from DB aggregates.

use super::*;
use crate::db::DbArtistSummary;

#[derive(Debug, Clone)]
pub struct ArtistSummary {
    pub raw: DbArtistSummary,
}

impl ArtistSummary {
    /// The artist as its row reads it, image and all.
    pub(crate) fn from_raw(raw: DbArtistSummary) -> Self {
        Self { raw }
    }
}

/// One existing library artist offered by an artist picker. Unlike the artist
/// browser, this includes rows without album links and keeps every exact ID a
/// person can use to distinguish otherwise identical names.
#[derive(Debug, Clone)]
pub struct ArtistSearchResult {
    pub artist: crate::db::DbArtist,
    pub image: Option<ImageRef>,
    /// The artist's other printed names, which the picker offers under it.
    pub credit_names: Vec<crate::credits::ArtistCreditName>,
}

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub artist: ArtistSummary,
    /// The artist's other printed names, which its page lists and edits.
    pub credit_names: Vec<crate::credits::ArtistCreditName>,
    pub albums: Vec<AlbumSummary>,
}
