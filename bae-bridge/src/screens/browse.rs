//! The library's lists as their screens pull them: album, artist and
//! composer browsing, the download queue, and the album grid's selection.
//! Each is one live query for as long as its screen is open, whose windows
//! or ids move in place.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::types::BridgeError;

/// Album, artist, and composer browsing are one subscription over a different
/// row. A macro rather than a generic: uniffi exports concrete objects and
/// records, so each row needs its own named type — the window and snapshot
/// records included, which is why they are declared here beside the conversion
/// that fills them.
macro_rules! browse_subscription {
    (
        object: $object:ident,
        inner: $inner:ty,
        built by $make:ident,
        snapshot: $snapshot:ident,
        window: $window:ident,
        row: $core_row:ty => $bridge_row:ty,
    ) => {
        /// One requested window of the list, with the rows that landed in it.
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $window {
            pub window: crate::types::BridgeLibraryPageWindow,
            pub rows: Vec<$bridge_row>,
        }

        /// Every requested window as of one revision, and what moved.
        #[derive(Debug, Clone, uniffi::Record)]
        pub struct $snapshot {
            pub windows: Vec<$window>,
            pub sections: Vec<crate::types::BridgeLibraryBrowseSection>,
            pub total_count: u64,
            pub row_count: u64,
            pub request_revision: u64,
            pub cause: crate::types::BridgeLiveQueryCause,
        }

        #[derive(uniffi::Object)]
        pub struct $object {
            inner: $inner,
            runtime: crate::operation_runtime::OperationRuntime,
        }

        pub(crate) fn $make(
            inner: $inner,
            runtime: crate::operation_runtime::OperationRuntime,
        ) -> Arc<$object> {
            Arc::new($object { inner, runtime })
        }

        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
            pub fn set_windows(
                &self,
                windows: Vec<crate::types::BridgeLibraryPageWindow>,
            ) -> Result<(), BridgeError> {
                self.inner.set_windows(core_windows(windows)).map_err(browse_error)
            }

            pub async fn next(self: Arc<Self>) -> Result<$snapshot, BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".next"), move || async move {
                        self.inner.next().await.map($snapshot::from_core).map_err(browse_error)
                    })
                    .await
            }

            pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
                let runtime = self.runtime.clone();
                runtime
                    .run(concat!(stringify!($object), ".cancel"), move || async move {
                        self.inner.cancel().await;
                        Ok(())
                    })
                    .await
            }
        }

        impl $snapshot {
            fn from_core(snapshot: bae_library::library::LibraryBrowseSnapshot<$core_row>) -> Self {
                Self {
                    windows: snapshot
                        .windows
                        .into_iter()
                        .map(|window| $window {
                            window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                            rows: window.rows.into_iter().map(<$bridge_row>::from_core).collect(),
                        })
                        .collect(),
                    total_count: snapshot.total_count,
                    row_count: snapshot.row_count,
                    sections: snapshot
                        .sections
                        .into_iter()
                        .map(|section| crate::types::BridgeLibraryBrowseSection {
                            id: section.id,
                            title: section.title,
                            image: section.image.map(crate::types::BridgeImageRef::from_core),
                            window: crate::types::BridgeLibraryPageWindow::from_core(
                                section.window,
                            ),
                        })
                        .collect(),
                    request_revision: snapshot.request_revision,
                    cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
                }
            }
        }
    };
}

browse_subscription! {
    object: AlbumBrowseSubscription,
    inner: bae_library::library::LiveAlbumBrowse,
    built by album_browse,
    snapshot: BridgeAlbumBrowseSnapshot,
    window: BridgeAlbumBrowseWindow,
    row: bae_library::album_detail::AlbumAppearance => crate::types::BridgeAlbumAppearance,
}

browse_subscription! {
    object: ArtistBrowseSubscription,
    inner: bae_library::library::LiveArtistBrowse,
    built by artist_browse,
    snapshot: BridgeArtistBrowseSnapshot,
    window: BridgeArtistBrowseWindow,
    row: bae_library::album_detail::ArtistSummary => crate::types::BridgeArtistSummary,
}

browse_subscription! {
    object: ComposerBrowseSubscription,
    inner: bae_library::library::LiveComposerBrowse,
    built by composer_browse,
    snapshot: BridgeComposerBrowseSnapshot,
    window: BridgeComposerBrowseWindow,
    row: bae_library::album_detail::ComposerSummary => crate::types::BridgeComposerSummary,
}

fn browse_error(error: bae_library::library::LiveLibraryBrowseError) -> BridgeError {
    match error {
        bae_library::library::LiveLibraryBrowseError::Cancelled => BridgeError::Cancelled,
        bae_library::library::LiveLibraryBrowseError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}

fn core_windows(
    windows: Vec<crate::types::BridgeLibraryPageWindow>,
) -> BTreeSet<bae_db::LibraryPageWindow> {
    windows.into_iter().map(crate::types::BridgeLibraryPageWindow::into_core).collect()
}

impl crate::types::BridgeLibraryPageWindow {
    pub(crate) fn from_core(window: bae_db::LibraryPageWindow) -> Self {
        Self { offset: window.offset, limit: window.limit }
    }

    pub(crate) fn into_core(self) -> bae_db::LibraryPageWindow {
        bae_db::LibraryPageWindow { offset: self.offset, limit: self.limit }
    }
}

mirror_enum! {
    crate::types::BridgeLiveQueryCause = coven::LiveQueryCause,
    from_core: pub(crate) fn,
    variants: { Request, Write, RequestAndWrite },
}

/// One requested window of the download queue, with its entries in the order
/// queued.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadBrowseWindow {
    pub window: crate::types::BridgeLibraryPageWindow,
    pub rows: Vec<crate::types::BridgeDownloadOp>,
}

/// Every requested window as of one read, and how many entries the queue
/// holds then.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadBrowseSnapshot {
    pub windows: Vec<BridgeDownloadBrowseWindow>,
    pub total_count: u64,
}

/// The Downloads pane's read of the download queue: how many entries it
/// holds, and the windows the pane draws of them, moved in place as it
/// scrolls. The pane's summary and controls are the queue's counts
/// ([`DownloadCountsSubscription`](crate::DownloadCountsSubscription)), and
/// each download's bytes its transfers.
#[derive(uniffi::Object)]
pub struct DownloadBrowseSubscription {
    inner: bae_library::library::LiveDownloadBrowse,
    runtime: crate::operation_runtime::OperationRuntime,
}

pub(crate) fn download_browse(
    inner: bae_library::library::LiveDownloadBrowse,
    runtime: crate::operation_runtime::OperationRuntime,
) -> Arc<DownloadBrowseSubscription> {
    Arc::new(DownloadBrowseSubscription { inner, runtime })
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl DownloadBrowseSubscription {
    /// Read `windows` of the queue from now on, offsets counted from its
    /// first entry.
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner.set_windows(core_windows(windows)).map_err(browse_error)
    }

    pub async fn next(self: Arc<Self>) -> Result<BridgeDownloadBrowseSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("DownloadBrowseSubscription.next", move || async move {
                self.inner
                    .next()
                    .await
                    .map(BridgeDownloadBrowseSnapshot::from_core)
                    .map_err(browse_error)
            })
            .await
    }

    pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("DownloadBrowseSubscription.cancel", move || async move {
                self.inner.cancel().await;
                Ok(())
            })
            .await
    }
}

impl BridgeDownloadBrowseSnapshot {
    fn from_core(
        snapshot: bae_library::library::LibraryBrowseSnapshot<bae_transfers::DownloadRow>,
    ) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| BridgeDownloadBrowseWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    rows: window
                        .rows
                        .into_iter()
                        .map(crate::types::BridgeDownloadOp::from_core)
                        .collect(),
                })
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

/// The album grid's multi-selection read live: one query for as long as the
/// grid is open, whose ids move in place as the selection changes. It starts
/// with none.
#[derive(uniffi::Object)]
pub struct AlbumSelectionSubscription {
    inner: bae_library::library::LiveAlbumSelection,
    runtime: crate::operation_runtime::OperationRuntime,
}

pub(crate) fn album_selection(
    inner: bae_library::library::LiveAlbumSelection,
    runtime: crate::operation_runtime::OperationRuntime,
) -> Arc<AlbumSelectionSubscription> {
    Arc::new(AlbumSelectionSubscription { inner, runtime })
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AlbumSelectionSubscription {
    /// Read `album_ids` from now on.
    pub fn set_albums(&self, album_ids: Vec<String>) -> Result<(), BridgeError> {
        self.inner.set_albums(album_ids).map_err(selection_error)
    }

    pub async fn next(self: Arc<Self>) -> Result<BridgeAlbumSelectionSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("AlbumSelectionSubscription.next", move || async move {
                self.inner
                    .next()
                    .await
                    .map(|snapshot| BridgeAlbumSelectionSnapshot {
                        requested: snapshot.requested.into_iter().collect(),
                        gone: snapshot.gone,
                    })
                    .map_err(selection_error)
            })
            .await
    }

    pub async fn cancel(self: Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("AlbumSelectionSubscription.cancel", move || async move {
                self.inner.cancel().await;
                Ok(())
            })
            .await
    }
}

fn selection_error(error: bae_library::library::LiveAlbumSelectionError) -> BridgeError {
    match error {
        bae_library::library::LiveAlbumSelectionError::Cancelled => BridgeError::Cancelled,
        bae_library::library::LiveAlbumSelectionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}
