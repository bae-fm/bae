use super::*;
use std::collections::BTreeSet;

/// Album browsing and composer browsing are one subscription over a different
/// row. A macro rather than a generic: uniffi exports concrete objects and
/// records, so each row needs its own named type.
macro_rules! browse_subscription {
    (
        object: $object:ident,
        inner: $inner:ty,
        subscribe: $subscribe:ident($criterion:ident),
        snapshot: $snapshot:ident,
        window: $window:ident,
        row: $core_row:ty => $row:path,
    ) => {
        #[derive(uniffi::Object)]
        pub struct $object {
            inner: $inner,
            runtime: tokio::runtime::Handle,
        }

        #[uniffi::export]
        impl AppHandle {
            pub fn $subscribe(&self, sort_criteria: Vec<$criterion>) -> std::sync::Arc<$object> {
                let sort = sort_criteria
                    .into_iter()
                    .map($criterion::into_core)
                    .collect::<Vec<_>>();
                std::sync::Arc::new($object {
                    inner: self.services.$subscribe(&sort),
                    runtime: self.runtime.handle().clone(),
                })
            }
        }

        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl $object {
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
            ) -> Result<crate::types::$snapshot, BridgeError> {
                let runtime = self.runtime.clone();
                crate::operation_runtime::run(runtime, move || async move {
                    self.inner
                        .next()
                        .await
                        .map(crate::types::$snapshot::from_core)
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

        impl crate::types::$snapshot {
            fn from_core(snapshot: bae_core::library::LibraryBrowseSnapshot<$core_row>) -> Self {
                Self {
                    windows: snapshot
                        .windows
                        .into_iter()
                        .map(|window| crate::types::$window {
                            window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                            rows: window.rows.into_iter().map($row).collect(),
                        })
                        .collect(),
                    total_count: snapshot.total_count,
                    request_revision: snapshot.request_revision,
                    cause: crate::types::BridgeLiveQueryCause::from_core(snapshot.cause),
                }
            }
        }
    };
}

browse_subscription! {
    object: AlbumBrowseSubscription,
    inner: bae_core::library::AlbumBrowseSubscription,
    subscribe: subscribe_album_browse(BridgeSortCriterion),
    snapshot: BridgeAlbumBrowseSnapshot,
    window: BridgeAlbumBrowseWindow,
    row: bae_core::album_detail::AlbumSummary => BridgeAlbum::from_core,
}

browse_subscription! {
    object: ComposerBrowseSubscription,
    inner: bae_core::library::ComposerBrowseSubscription,
    subscribe: subscribe_composer_browse(BridgeComposerSortCriterion),
    snapshot: BridgeComposerBrowseSnapshot,
    window: BridgeComposerBrowseWindow,
    row: bae_core::album_detail::ComposerSummary => BridgeComposerSummary::from_core,
}

fn browse_error(error: bae_core::library::LibraryBrowseSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::LibraryBrowseSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LibraryBrowseSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
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
    pub(super) fn from_core(window: bae_core::library::LibraryPageWindow) -> Self {
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

impl crate::types::BridgeLiveQueryCause {
    pub(super) fn from_core(cause: coven::ReconfigurableLiveQueryCause) -> Self {
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
