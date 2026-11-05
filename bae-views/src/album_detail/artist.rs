//! Resolved artist types and their pure projections from DB aggregates.

use super::*;
use crate::DbArtistSummary;

#[derive(Debug, Clone)]
pub struct ArtistSummary {
    pub raw: DbArtistSummary,
}

impl ArtistSummary {
    /// The artist as its row reads it, image and all.
    pub(crate) fn from_raw(raw: DbArtistSummary) -> Self {
        Self { raw }
    }

    /// The line under the artist's name in its list, its detail and search:
    /// how many albums credit it.
    pub fn count_line(&self) -> bae_types::count_label::CountLabel {
        bae_types::count_label::CountLabel {
            key: "core.library.album_count".to_string(),
            count: u32::try_from(self.raw.album_count).expect("an album count is never negative"),
        }
    }
}

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
    pub artist: ArtistSummary,
    /// The artist's other printed names, which its page lists and edits.
    pub credit_names: Vec<bae_records::credits::ArtistCreditName>,
    pub albums: Vec<AlbumSummary>,
}
