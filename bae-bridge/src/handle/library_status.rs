use super::*;

/// One release an import pane offers, as the library-membership check reads
/// it: the catalog's own release id and, when the catalog surfaces one, its
/// group id.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryCheck {
    pub source: crate::types::BridgeCatalog,
    pub release_id: String,
    pub source_group_id: Option<String>,
}

/// The library membership of every checked release, by release id, as of
/// one revision of the checks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatusSnapshot {
    pub statuses: std::collections::HashMap<String, crate::types::BridgeLibraryStatus>,
    /// The revision of the `set_checks` call these statuses answer.
    pub request_revision: u64,
}

/// Whether the releases an import pane offers are already in the library:
/// one live query for as long as the pane is open, whose checks move in place
/// as its offers change. It starts with no checks.
#[derive(uniffi::Object)]
pub struct LibraryStatusSubscription {
    inner: bae_core::library::LibraryStatusSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_library_statuses(&self) -> std::sync::Arc<LibraryStatusSubscription> {
        std::sync::Arc::new(LibraryStatusSubscription {
            inner: self.services.subscribe_library_statuses(),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl LibraryStatusSubscription {
    /// Check `checks` from now on. Returns the revision the answer will
    /// carry.
    pub fn set_checks(&self, checks: Vec<BridgeLibraryCheck>) -> Result<u64, BridgeError> {
        self.inner
            .set_checks(checks.into_iter().map(|check| bae_core::db::LibraryCheck {
                release_id: check.release_id,
                source: check.source.into_core(),
                source_group_id: check.source_group_id,
            }))
            .map_err(status_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeLibraryStatusSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| BridgeLibraryStatusSnapshot {
                    statuses: snapshot
                        .statuses
                        .into_iter()
                        .map(|status| {
                            (
                                status.release_id.clone(),
                                crate::types::BridgeLibraryStatus::from_core(status),
                            )
                        })
                        .collect(),
                    request_revision: snapshot.request_revision,
                })
                .map_err(status_error)
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

fn status_error(error: bae_core::library::LibraryStatusSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::LibraryStatusSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::LibraryStatusSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}
