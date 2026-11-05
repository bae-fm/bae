//! The album grid's multi-selection, read through one live query whose ids
//! change as the selection does.

use bae_store::live_query::{CancellableLiveQuery, LiveQueryError};
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
pub struct LiveAlbumSelection {
    query: CancellableLiveQuery<BTreeSet<String>, Vec<String>>,
}

impl LiveAlbumSelection {
    pub fn new(query: CancellableLiveQuery<BTreeSet<String>, Vec<String>>) -> Self {
        Self { query }
    }

    /// Read `album_ids` from now on.
    pub fn set_albums(
        &self,
        album_ids: impl IntoIterator<Item = String>,
    ) -> Result<(), LiveQueryError> {
        self.query.set(album_ids.into_iter().collect()).map(|_| ()).map_err(LiveQueryError::from)
    }

    pub async fn next(&self) -> Result<AlbumSelectionSnapshot, LiveQueryError> {
        let event = self.query.next().await.map_err(LiveQueryError::from)?;
        Ok(AlbumSelectionSnapshot { requested: event.request.clone(), gone: event.result? })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
