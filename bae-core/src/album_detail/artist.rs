//! Resolved artist types and their pure projections from DB aggregates.

use super::*;
use crate::db::DbArtistSummary;

#[derive(Debug, Clone)]
pub struct ArtistSummary {
    pub raw: DbArtistSummary,
    pub image: Option<ImageRef>,
}

impl ArtistSummary {
    pub(crate) fn from_raw(raw: DbArtistSummary, image: Option<ImageRef>) -> Self {
        Self { raw, image }
    }
}

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub artist: ArtistSummary,
    pub albums: Vec<AlbumSummary>,
}
