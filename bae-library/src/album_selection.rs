//! The album grid's multi-selection, read through one live query whose ids
//! change as the selection does.

use bae_store::live_query::LiveRead;
use std::collections::BTreeSet;

/// One value the subscription delivered: the ids it read, and those of them
/// the library no longer shows, by id.
#[derive(Debug, Clone)]
pub struct AlbumSelectionSnapshot {
    pub requested: BTreeSet<String>,
    pub gone: Vec<String>,
}

/// Which selected albums are gone from the library, wherever in the grid
/// they sit — what leaves the selection. A new selection is a new request on
/// the same query, and one read per database change answers every selected
/// album.
pub type LiveAlbumSelection = LiveRead<BTreeSet<String>, AlbumSelectionSnapshot>;
