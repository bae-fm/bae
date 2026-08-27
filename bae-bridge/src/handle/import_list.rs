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

#[uniffi::export(async_runtime = "tokio", cancellable)]
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

    pub async fn locate_import_candidate(
        self: std::sync::Arc<Self>,
        view: crate::types::BridgeImportListView,
        candidate_key: String,
    ) -> Result<Option<crate::types::BridgeImportCandidateListLocation>, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .locate_import_candidate(view.into_core(), &candidate_key)
                .await
                .map(|location| {
                    location.map(crate::types::BridgeImportCandidateListLocation::from_core)
                })
                .map_err(BridgeError::database_query)
        })
        .await
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
                    Err(error) => callback.on_error(BridgeError::database_query(error)),
                }
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// What is in flight for one key right now — the read a view does once
    /// when it appears, after it has subscribed to the changes.
    pub fn candidate_runtime(
        &self,
        candidate_key: String,
    ) -> Option<crate::types::BridgeCandidateRuntimeSnapshot> {
        self.services
            .candidate_runtime(&candidate_key)
            .map(crate::types::BridgeCandidateRuntimeSnapshot::from_core)
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the UI bus. `None`
    /// before the first snapshot, and for a run that settled in an earlier
    /// session: what that run stored is on the candidate's row instead.
    pub fn candidate_signals(&self, candidate_key: String) -> Option<crate::types::BridgeSignals> {
        self.services
            .candidate_signals(&candidate_key)
            .map(crate::types::BridgeSignals::from_core)
    }

    /// What every candidate has in flight: a run's identify state and a
    /// running import's progress, keyed by candidate.
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
                             re-stating every key in flight"
                        );
                        callback.on_change(crate::types::BridgeCandidateRuntimeChange::reset(
                            services.candidate_runtimes(),
                        ));
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
    pub fn set_view(&self, view: crate::types::BridgeImportListView) -> Result<u64, BridgeError> {
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
        bae_core::import::ImportListSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}
