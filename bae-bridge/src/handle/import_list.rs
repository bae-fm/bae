use super::*;
use std::collections::BTreeSet;

/// The import tab's list, reconfigurable by view and by window.
///
/// The same shape as [`AlbumBrowseSubscription`](super::AlbumBrowseSubscription):
/// one object per list, `set_view` and `set_windows` reconfigure it, `next`
/// waits for the value that answers.
#[derive(uniffi::Object)]
pub struct ImportListSubscription {
    inner: bae_core::import::ImportListSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_import_list(
        &self,
        view: crate::types::BridgeImportListView,
    ) -> std::sync::Arc<ImportListSubscription> {
        let runtime = self.runtime.handle().clone();
        std::sync::Arc::new(ImportListSubscription {
            inner: self
                .services
                .subscribe_import_list(view.into_core(), &runtime),
            runtime,
        })
    }

    /// One candidate as the pane reads it, and every later read of it.
    pub fn subscribe_import_candidate(
        &self,
        candidate_key: String,
        callback: Box<dyn crate::types::ImportCandidateCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let service_runtime = runtime.clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values =
                services.subscribe_import_candidate_values(&service_runtime, candidate_key);
            while let Some(value) = values.recv().await {
                match value {
                    Ok(value) => callback
                        .on_value(value.map(crate::types::BridgeImportCandidateDetail::from_core)),
                    Err(error) => callback.on_error(BridgeError::database(error)),
                }
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// Every candidate's runtime: what a run in flight and an import in
    /// progress are doing, keyed by candidate.
    pub fn subscribe_candidate_runtime(
        &self,
        callback: Box<dyn crate::types::CandidateRuntimeCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let (initial, mut changes) = services.subscribe_candidate_runtime();
            for (key, runtime) in initial {
                callback.on_change(crate::types::BridgeCandidateRuntimeChange::Updated {
                    key,
                    runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
                });
            }
            loop {
                match changes.recv().await {
                    Ok(change) => callback.on_change(
                        crate::types::BridgeCandidateRuntimeChange::from_core(change),
                    ),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!(
                            "candidate runtime subscription dropped {count} changes; \
                             re-sending every key's runtime"
                        );
                        for (key, runtime) in services.subscribe_candidate_runtime().0 {
                            callback.on_change(
                                crate::types::BridgeCandidateRuntimeChange::Updated {
                                    key,
                                    runtime:
                                        crate::types::BridgeCandidateRuntimeSnapshot::from_core(
                                            runtime,
                                        ),
                                },
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ImportListSubscription {
    pub fn set_view(&self, view: crate::types::BridgeImportListView) -> Result<(), BridgeError> {
        self.inner.set_view(view.into_core()).map_err(list_error)
    }

    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        let windows: BTreeSet<bae_core::library::LibraryPageWindow> = windows
            .into_iter()
            .map(|window| bae_core::library::LibraryPageWindow {
                offset: window.offset,
                limit: window.limit,
            })
            .collect();
        self.inner.set_windows(windows).map_err(list_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeImportListSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(crate::types::BridgeImportListSnapshot::from_core)
                .map_err(list_error)
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

fn list_error(error: bae_core::import::ImportListSubscriptionError) -> BridgeError {
    match error {
        bae_core::import::ImportListSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::import::ImportListSubscriptionError::Query(error) => BridgeError::database(error),
    }
}
