use super::detail::live_read_error;
use super::*;

/// One requested window of the Storage Manager list, with its rows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageBrowseWindow {
    pub window: crate::types::BridgeLibraryPageWindow,
    pub rows: Vec<BridgeStorageRow>,
}

/// Every requested window of the Storage Manager list under the sort and
/// filter it was read for — a UI that has since moved to another drops it —
/// with the filtered set's row count and total size.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageBrowseSnapshot {
    pub sort: BridgeStorageSort,
    pub filter: BridgeStorageFilter,
    pub windows: Vec<BridgeStorageBrowseWindow>,
    pub total_count: u64,
    pub total_size: u64,
}

/// The Storage Manager list read live: one query for as long as the manager
/// is open, whose sort, filter, and windows move in place.
#[derive(uniffi::Object)]
pub struct StorageBrowseSubscription {
    inner: bae_core::library::StorageBrowseSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    /// A subscription to the list under `sort` and `filter`, reading no
    /// windows until `set_view` names some.
    pub fn subscribe_storage_browse(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
    ) -> std::sync::Arc<StorageBrowseSubscription> {
        std::sync::Arc::new(StorageBrowseSubscription {
            inner: self.services.subscribe_storage_browse(
                &self.runtime,
                bae_core::library::StorageBrowseView {
                    sort: sort.into_core(),
                    filter: filter.into_core(),
                    windows: Default::default(),
                },
            ),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl StorageBrowseSubscription {
    /// Read `windows` of the list under `sort` and `filter` from now on.
    pub fn set_view(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(bae_core::library::StorageBrowseView {
                sort: sort.into_core(),
                filter: filter.into_core(),
                windows: windows
                    .into_iter()
                    .map(crate::types::BridgeLibraryPageWindow::into_core)
                    .collect(),
            })
            .map_err(live_read_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeStorageBrowseSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| BridgeStorageBrowseSnapshot {
                    sort: BridgeStorageSort::from_core(snapshot.sort),
                    filter: BridgeStorageFilter::from_core(snapshot.filter),
                    windows: snapshot
                        .windows
                        .into_iter()
                        .map(|window| BridgeStorageBrowseWindow {
                            window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                            rows: window
                                .rows
                                .into_iter()
                                .map(BridgeStorageRow::from_core)
                                .collect(),
                        })
                        .collect(),
                    total_count: snapshot.total_count,
                    total_size: snapshot.total_size,
                })
                .map_err(live_read_error)
        })
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner.cancel();
            Ok(())
        })
        .await
    }
}
