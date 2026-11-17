//! This module reads the album grid's selection live.

use crate::types::BridgeError;

mirror_enum! {
    crate::types::BridgeLiveQueryCause = coven::LiveQueryCause,
    from_core: pub(crate) fn,
    variants: { Request, Write, RequestAndWrite },
}

/// One value an album selection delivered: the ids it read, and those of them
/// the library no longer shows, by id.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSelectionSnapshot {
    pub requested: Vec<String>,
    pub gone: Vec<String>,
}

impl BridgeAlbumSelectionSnapshot {
    fn from_core(snapshot: bae_library::AlbumSelectionSnapshot) -> Self {
        Self { requested: snapshot.requested.into_iter().collect(), gone: snapshot.gone }
    }
}

pulled_read! {
    /// The album grid's multi-selection read live: one query for as long as
    /// the grid is open, whose ids move in place as the selection changes. It
    /// starts with none.
    AlbumSelectionSubscription: bae_library::LiveAlbumSelection => BridgeAlbumSelectionSnapshot,
    converted by BridgeAlbumSelectionSnapshot::from_core,
    built by album_selection
}

#[uniffi::export]
impl AlbumSelectionSubscription {
    /// Read `album_ids` from now on.
    pub fn set_albums(&self, album_ids: Vec<String>) -> Result<(), BridgeError> {
        self.inner
            .set(album_ids.into_iter().collect())
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}
