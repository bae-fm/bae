//! Resolved artist types and their pure projections from DB aggregates.

use super::*;
use crate::DbArtistSummary;

/// One existing library artist offered by an artist picker. Unlike the artist
/// browser, this includes rows without album links and keeps every exact ID a
/// person can use to distinguish otherwise identical names.
#[derive(Debug, Clone)]
pub struct ArtistSearchResult {
    pub artist: bae_records::DbArtist,
    pub image: Option<ImageRef>,
    /// The artist's other printed names, which the picker offers under it.
    pub credit_names: Vec<bae_records::credits::ArtistCreditName>,
}

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub artist: DbArtistSummary,
    /// The artist's other printed names, which its page lists and edits.
    pub credit_names: Vec<bae_records::credits::ArtistCreditName>,
    pub albums: Vec<AlbumSummary>,
}
