//! The library's lists as their screens pull them: album, artist and
//! composer browsing, a composer's page, the download queue, and the album
//! grid's selection.
//! Each is one live query for as long as its screen is open, whose windows
//! or ids move in place.

use crate::types::BridgeError;

/// Every windowed list is one subscription object over its own snapshot. A
/// macro rather than a generic: uniffi exports concrete objects and records,
/// so each list needs its own named types.
macro_rules! browse_subscription {
    (
        $(#[$meta:meta])*
        object: $object:ident,
        inner: $inner:ty,
        built by $make:ident,
        snapshot: $snapshot:ident,
    ) => {
        pulled_read! {
            $(#[$meta])*
            $object: $inner => $snapshot, converted by $snapshot::from_core, built by $make
        }

        #[uniffi::export]
        impl $object {
            /// Read `windows` of the list from now on, offsets counted from
            /// its first row.
            pub fn set_windows(
                &self,
                windows: Vec<crate::types::BridgeLibraryPageWindow>,
            ) -> Result<(), BridgeError> {
                self.inner
                    .set(super::core_windows(windows))
                    .map(|_revision| ())
                    .map_err(BridgeError::from)
            }
        }
    };
}

/// A list's snapshot that carries its windows and its row count and nothing
/// beside them.
macro_rules! browse_snapshot {
    (snapshot: $snapshot:ident, window: $window:ident, row: $core_row:ty => $bridge_row:ty,) => {
        browse_window! {
            /// One requested window of the list, with the rows that landed in it.
            window: $window,
            row: $core_row => $bridge_row,
        }

        /// Every requested window as of one revision.
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $snapshot {
            pub windows: Vec<$window>,
            pub total_count: u64,
            pub request_revision: u64,
            pub cause: crate::types::BridgeLiveQueryCause,
        }

        impl $snapshot {
            fn from_core(snapshot: bae_views::LibraryBrowseSnapshot<$core_row>) -> Self {
                Self {
                    windows: snapshot.windows.into_iter().map($window::from_core).collect(),
                    total_count: snapshot.total_count,
                    request_revision: snapshot.request_revision,
                    cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
                }
            }
        }
    };
}

browse_window! {
    /// One requested window of the album grid, with the albums that landed in
    /// it.
    window: BridgeAlbumBrowseWindow,
    row: bae_views::album_detail::AlbumAppearance => crate::types::BridgeAlbumAppearance,
}

/// The album grid as of one revision. Grouped by artist, an album shows once
/// in each of its artists' sections, so the grid's positions count
/// appearances: `row_count` of them, over `total_count` albums. Flat, the two
/// are equal.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumBrowseSnapshot {
    pub windows: Vec<BridgeAlbumBrowseWindow>,
    pub total_count: u64,
    pub row_count: u64,
    pub request_revision: u64,
    pub cause: crate::types::BridgeLiveQueryCause,
}

impl BridgeAlbumBrowseSnapshot {
    fn from_core(snapshot: bae_views::AlbumBrowseSnapshot) -> Self {
        let bae_views::AlbumBrowseSnapshot { list, row_count } = snapshot;
        Self {
            windows: list.windows.into_iter().map(BridgeAlbumBrowseWindow::from_core).collect(),
            total_count: list.total_count,
            row_count,
            request_revision: list.request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(list.cause),
        }
    }
}

browse_subscription! {
    object: AlbumBrowseSubscription,
    inner: bae_views::LiveAlbumBrowse,
    built by album_browse,
    snapshot: BridgeAlbumBrowseSnapshot,
}

browse_snapshot! {
    snapshot: BridgeArtistBrowseSnapshot,
    window: BridgeArtistBrowseWindow,
    row: bae_views::DbArtistSummary => crate::types::BridgeArtistSummary,
}

browse_subscription! {
    object: ArtistBrowseSubscription,
    inner: bae_views::LiveArtistBrowse,
    built by artist_browse,
    snapshot: BridgeArtistBrowseSnapshot,
}

browse_snapshot! {
    snapshot: BridgeComposerBrowseSnapshot,
    window: BridgeComposerBrowseWindow,
    row: bae_views::DbComposerSummary => crate::types::BridgeComposerSummary,
}

browse_subscription! {
    object: ComposerBrowseSubscription,
    inner: bae_views::LiveComposerBrowse,
    built by composer_browse,
    snapshot: BridgeComposerBrowseSnapshot,
}

browse_snapshot! {
    snapshot: BridgeComposerDetailRowsSnapshot,
    window: BridgeComposerDetailRowsWindow,
    row: bae_views::album_detail::ComposerDetailRow => crate::types::BridgeComposerDetailRow,
}

browse_subscription! {
    object: ComposerDetailRowsSubscription,
    inner: bae_views::LiveComposerDetailRows,
    built by composer_detail_rows,
    snapshot: BridgeComposerDetailRowsSnapshot,
}

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

    fn into_core(self) -> bae_store::list_windows::SortValue {
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

browse_window! {
    /// One requested window of the download queue, with its entries in the
    /// order queued.
    window: BridgeDownloadBrowseWindow,
    row: bae_transfers::DownloadRow => crate::types::BridgeDownloadOp,
}

/// Every requested window as of one read, and how many entries the queue
/// holds then.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadBrowseSnapshot {
    pub windows: Vec<BridgeDownloadBrowseWindow>,
    pub total_count: u64,
}

browse_subscription! {
    /// The Downloads pane's read of the download queue: how many entries it
    /// holds, and the windows the pane draws of them, moved in place as it
    /// scrolls. The pane's summary and controls are the queue's counts
    /// ([`DownloadCountsSubscription`](crate::DownloadCountsSubscription)), and
    /// each download's bytes its transfers.
    object: DownloadBrowseSubscription,
    inner: bae_views::LiveDownloadBrowse,
    built by download_browse,
    snapshot: BridgeDownloadBrowseSnapshot,
}

impl BridgeDownloadBrowseSnapshot {
    fn from_core(snapshot: bae_views::LibraryBrowseSnapshot<bae_transfers::DownloadRow>) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(BridgeDownloadBrowseWindow::from_core)
                .collect(),
            total_count: snapshot.total_count,
        }
    }
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
