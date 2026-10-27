//! The Downloads pane's read of the download queue: how many entries it
//! holds, and the windows the pane draws of them, moved in place as it
//! scrolls. The pane's summary and controls are the queue's counts
//! (`subscribe_download_counts`), and each download's bytes its transfers.

use super::*;

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

#[derive(uniffi::Object)]
pub struct DownloadBrowseSubscription {
    inner: bae_core::library::DownloadBrowseSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    /// A read of the download queue that reads no windows until
    /// `set_windows`.
    pub fn subscribe_download_browse(&self) -> std::sync::Arc<DownloadBrowseSubscription> {
        std::sync::Arc::new(DownloadBrowseSubscription {
            inner: self.services.subscribe_download_browse(),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl DownloadBrowseSubscription {
    /// Read `windows` of the queue from now on, offsets counted from its
    /// first entry.
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set_windows(
                windows
                    .into_iter()
                    .map(crate::types::BridgeLibraryPageWindow::into_core)
                    .collect(),
            )
            .map_err(super::collection_subscription::browse_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeDownloadBrowseSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(BridgeDownloadBrowseSnapshot::from_core)
                .map_err(super::collection_subscription::browse_error)
        })
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner.cancel().await;
            Ok(())
        })
        .await
    }
}

impl BridgeDownloadBrowseSnapshot {
    fn from_core(
        snapshot: bae_core::library::LibraryBrowseSnapshot<bae_core::library::DownloadRow>,
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
