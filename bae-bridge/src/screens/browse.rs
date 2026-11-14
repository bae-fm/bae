//! The library's list screens' other reads: the album grid's selection, and
//! the conversions a window's sort values take across the bridge.

use crate::types::BridgeError;

impl crate::types::BridgeLibraryPageWindow {
    pub(crate) fn from_core(window: bae_store::LibraryPageWindow) -> Self {
        Self {
            offset: window.offset,
            limit: window.limit,
            cursor: window.cursor.map(crate::types::BridgeWindowCursor::from_core),
        }
    }

    pub(crate) fn into_core(self) -> bae_store::LibraryPageWindow {
        bae_store::LibraryPageWindow {
            offset: self.offset,
            limit: self.limit,
            cursor: self.cursor.map(crate::types::BridgeWindowCursor::into_core),
        }
    }
}

impl crate::types::BridgeWindowCursor {
    fn from_core(cursor: bae_store::list_windows::Cursor) -> Self {
        use bae_store::list_windows::Cursor;
        let (side, key) = match cursor {
            Cursor::After(key) => (crate::types::BridgeCursorSide::After, key),
            Cursor::Before(key) => (crate::types::BridgeCursorSide::Before, key),
        };
        Self { side, key: crate::types::BridgeSortValue::of_key(&key) }
    }

    fn into_core(self) -> bae_store::list_windows::Cursor {
        use bae_store::list_windows::{Cursor, OrderKey};
        let key = OrderKey::new(
            self.key.into_iter().map(crate::types::BridgeSortValue::into_core).collect(),
        );
        match self.side {
            crate::types::BridgeCursorSide::After => Cursor::After(key),
            crate::types::BridgeCursorSide::Before => Cursor::Before(key),
        }
    }
}

impl crate::types::BridgeSortValue {
    /// The values of `key`, which a screen sends back as a cursor.
    pub(crate) fn of_key(key: &bae_store::list_windows::OrderKey) -> Vec<Self> {
        use bae_store::list_windows::SortValue;
        key.values()
            .iter()
            .map(|value| match value {
                SortValue::Null => Self::Null,
                SortValue::Integer(value) => Self::Integer { value: *value },
                SortValue::Real(value) => Self::Real { value: *value },
                SortValue::Text(value) => Self::Text { value: value.clone() },
                SortValue::Blob(value) => Self::Blob { value: value.clone() },
            })
            .collect()
    }

    pub(crate) fn into_core(self) -> bae_store::list_windows::SortValue {
        use bae_store::list_windows::SortValue;
        match self {
            Self::Null => SortValue::Null,
            Self::Integer { value } => SortValue::Integer(value),
            Self::Real { value } => SortValue::Real(value),
            Self::Text { value } => SortValue::Text(value),
            Self::Blob { value } => SortValue::Blob(value),
        }
    }
}

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
