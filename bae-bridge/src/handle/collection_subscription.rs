use super::*;
use std::collections::BTreeSet;

#[derive(uniffi::Object)]
pub struct AlbumBrowseSubscription {
    inner: bae_core::library::AlbumBrowseSubscription,
    runtime: tokio::runtime::Handle,
}

#[derive(uniffi::Object)]
pub struct ComposerBrowseSubscription {
    inner: bae_core::library::ComposerBrowseSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_album_browse(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
    ) -> std::sync::Arc<AlbumBrowseSubscription> {
        let sort = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect::<Vec<_>>();
        std::sync::Arc::new(AlbumBrowseSubscription {
            inner: self.services.subscribe_album_browse(&sort),
            runtime: self.runtime.handle().clone(),
        })
    }

    pub fn subscribe_composer_browse(
        &self,
        sort_criteria: Vec<BridgeComposerSortCriterion>,
    ) -> std::sync::Arc<ComposerBrowseSubscription> {
        let sort = sort_criteria
            .into_iter()
            .map(BridgeComposerSortCriterion::into_core)
            .collect::<Vec<_>>();
        std::sync::Arc::new(ComposerBrowseSubscription {
            inner: self.services.subscribe_composer_browse(&sort),
            runtime: self.runtime.handle().clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AlbumBrowseSubscription {
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set_windows(core_windows(windows))
            .map_err(browse_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeAlbumBrowseSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(crate::types::BridgeAlbumBrowseSnapshot::from_core)
                .map_err(browse_error)
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

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ComposerBrowseSubscription {
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set_windows(core_windows(windows))
            .map_err(browse_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeComposerBrowseSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(crate::types::BridgeComposerBrowseSnapshot::from_core)
                .map_err(browse_error)
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

fn browse_error(error: bae_core::library::LibraryBrowseSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::LibraryBrowseSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LibraryBrowseSubscriptionError::Query(error) => {
            BridgeError::database(error)
        }
    }
}

fn core_windows(
    windows: Vec<crate::types::BridgeLibraryPageWindow>,
) -> BTreeSet<bae_core::library::LibraryPageWindow> {
    windows
        .into_iter()
        .map(crate::types::BridgeLibraryPageWindow::into_core)
        .collect()
}

impl crate::types::BridgeLibraryPageWindow {
    fn from_core(window: bae_core::library::LibraryPageWindow) -> Self {
        Self {
            offset: window.offset,
            limit: window.limit,
        }
    }

    fn into_core(self) -> bae_core::library::LibraryPageWindow {
        bae_core::library::LibraryPageWindow {
            offset: self.offset,
            limit: self.limit,
        }
    }
}

impl crate::types::BridgeAlbumBrowseSnapshot {
    fn from_core(
        snapshot: bae_core::library::LibraryBrowseSnapshot<bae_core::album_detail::AlbumSummary>,
    ) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| crate::types::BridgeAlbumBrowseWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    rows: window
                        .rows
                        .into_iter()
                        .map(BridgeAlbum::from_core)
                        .collect(),
                })
                .collect(),
            total_count: snapshot.total_count,
            request_revision: snapshot.request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
        }
    }
}

impl crate::types::BridgeComposerBrowseSnapshot {
    fn from_core(
        snapshot: bae_core::library::LibraryBrowseSnapshot<bae_core::album_detail::ComposerSummary>,
    ) -> Self {
        Self {
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| crate::types::BridgeComposerBrowseWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    rows: window
                        .rows
                        .into_iter()
                        .map(BridgeComposerSummary::from_core)
                        .collect(),
                })
                .collect(),
            total_count: snapshot.total_count,
            request_revision: snapshot.request_revision,
            cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
        }
    }
}

impl crate::types::BridgeLiveQueryCause {
    fn from_core(cause: coven::ReconfigurableLiveQueryCause) -> Self {
        match cause {
            coven::ReconfigurableLiveQueryCause::Initial => Self::Initial,
            coven::ReconfigurableLiveQueryCause::RequestChanged => Self::RequestChanged,
            coven::ReconfigurableLiveQueryCause::DatabaseChanged => Self::DatabaseChanged,
            coven::ReconfigurableLiveQueryCause::RequestAndDatabaseChanged => {
                Self::RequestAndDatabaseChanged
            }
        }
    }
}
