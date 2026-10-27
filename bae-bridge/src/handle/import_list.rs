use super::*;
use std::collections::BTreeSet;

forward! { async this => {
    fn locate_import_candidate(
        view: crate::types::BridgeImportListView,
        candidate_key: String,
    ) -> Option<crate::types::BridgeImportCandidateListLocation> {
        this.services
            .locate_import_candidate(view.into_core(), &candidate_key)
            .await
            .map(|location| {
                location.map(crate::types::BridgeImportCandidateListLocation::from_core)
            })
            .map_err(BridgeError::database_query)
    }

    fn first_identifying_candidate(
        view: crate::types::BridgeImportListView,
    ) -> Option<String> {
        this.services
            .first_identifying_candidate(view.into_core())
            .await
            .map_err(BridgeError::database_query)
    }

    /// Record the person's answer to the candidate's artist conflict.
    fn choose_candidate_artist_identity(
        candidate_key: String,
        metadata_revision: u64,
        choice: crate::types::BridgeArtistIdentityConflictChoice,
    ) -> crate::types::BridgePaneOutcome {
        Ok(crate::types::BridgePaneOutcome::from_core(
            this.services
                .pane_choose_artist_identity(&candidate_key, metadata_revision, choice.into_core())
                .await?,
        ))
    }
} }

/// The import tab's list, reconfigurable by view and by window, and the
/// selection it owns.
///
/// The same shape as [`AlbumBrowseSubscription`](super::AlbumBrowseSubscription):
/// one object per list, `set_view` and `set_windows` reconfigure it, `next`
/// waits for the value that answers. The selection only ever holds rows the
/// list shows, so changing it and acting on it are this list's to do.
#[derive(uniffi::Object)]
pub struct ImportListSubscription {
    inner: bae_core::import::ImportListSubscription,
    services: AppServices,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_import_list(
        &self,
        view: crate::types::BridgeImportListView,
    ) -> std::sync::Arc<ImportListSubscription> {
        let runtime = self.runtime.clone();
        std::sync::Arc::new(ImportListSubscription {
            inner: self
                .services
                .subscribe_import_list(view.into_core(), &runtime),
            services: self.services.clone(),
            runtime,
        })
    }

    /// What is in flight for one key right now — the read a view does once
    /// when it appears, after it has subscribed to the changes. It is the
    /// value the runtime stream last delivered, so the two agree.
    pub fn candidate_runtime(
        &self,
        candidate_key: String,
    ) -> Option<crate::types::BridgeCandidateRuntimeSnapshot> {
        self.services
            .latest_candidate_runtime(&candidate_key)
            .map(crate::types::BridgeCandidateRuntimeSnapshot::from_core)
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the UI events. `None`
    /// before the first snapshot, and for a run that settled in an earlier
    /// session: what that run stored is on the candidate's row instead.
    pub fn candidate_signals(&self, candidate_key: String) -> Option<crate::types::BridgeSignals> {
        self.services
            .candidate_signals(&candidate_key)
            .map(crate::types::BridgeSignals::from_core)
    }

    /// Where each watched folder's scan stands, now and as it changes.
    pub fn subscribe_folder_scans(
        &self,
        callback: Box<dyn crate::types::FolderScansCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            |services| services.subscribe_folder_scans(),
            move |value| match value {
                Ok(value) => callback.on_value(crate::types::BridgeFolderScans::from_core(value)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    /// What every candidate has in flight: a run's identify state and a
    /// running import's progress, keyed by candidate.
    pub fn subscribe_candidate_runtime(
        &self,
        callback: Box<dyn crate::types::CandidateRuntimeCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services| async move {
            let mut runtimes = services.watch_candidate_runtimes();
            while let Some(changes) = runtimes.next().await {
                callback.on_changes(
                    changes
                        .into_iter()
                        .map(crate::types::BridgeCandidateRuntimeChange::from_core)
                        .collect(),
                );
            }
        })
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

    /// Apply a change a person made by pointing at rows of the list, and
    /// return the selection revision of the list value that reflects it.
    pub fn change_selection(
        &self,
        change: crate::types::BridgeSelectionChange,
    ) -> Result<u64, BridgeError> {
        self.inner
            .change_selection(change.into_core())
            .map_err(list_error)
    }

    /// Select every row the list shows, loaded by the surface or not, and
    /// return the selection revision of the list value that reflects it.
    pub fn select_all(&self) -> Result<u64, BridgeError> {
        self.inner.select_all().map_err(list_error)
    }

    /// The folders of every selected candidate.
    pub async fn selection_source_folders(
        self: std::sync::Arc<Self>,
    ) -> Result<Vec<String>, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.services
                .import_selection_source_folders(&self.inner.selected())
                .await
                .map_err(BridgeError::from)
        })
        .await
    }

    /// Read every selected candidate as one release, select it, and answer
    /// with its key.
    pub async fn combine_selection(self: std::sync::Arc<Self>) -> Result<String, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let key = self
                .services
                .combine_import_selection(&self.inner.selected())
                .await
                .map_err(BridgeError::from)?;
            self.inner
                .change_selection(bae_core::import::selection::SelectionChange::Replace {
                    keys: vec![key.clone()],
                })
                .map_err(list_error)?;
            Ok(key)
        })
        .await
    }

    /// Read the one selected release as the releases it is made of, select
    /// them, and answer with their keys.
    pub async fn separate_selection(
        self: std::sync::Arc<Self>,
    ) -> Result<Vec<String>, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let keys = self
                .services
                .separate_import_selection(&self.inner.selected())
                .await
                .map_err(BridgeError::from)?;
            self.inner
                .change_selection(bae_core::import::selection::SelectionChange::Replace {
                    keys: keys.clone(),
                })
                .map_err(list_error)?;
            Ok(keys)
        })
        .await
    }

    /// Run `action` on every selected candidate that offers it, in the order
    /// the list shows them, reporting how far it has got, and answer with the
    /// candidates it failed on.
    pub async fn run_selection_action(
        self: std::sync::Arc<Self>,
        action: crate::types::BridgeCandidateAction,
        progress: Box<dyn crate::types::SelectionActionProgressCallback>,
    ) -> Result<Vec<crate::types::BridgeSelectionActionFailure>, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let failures = self
                .services
                .run_import_selection_action(
                    &self.inner.selected(),
                    action.into_core(),
                    |completed, total| {
                        progress.on_progress(crate::types::BridgeSelectionActionProgress {
                            completed,
                            total,
                        })
                    },
                )
                .await
                .map_err(BridgeError::from)?;
            Ok(failures
                .into_iter()
                .map(|failure| crate::types::BridgeSelectionActionFailure {
                    candidate_key: failure.candidate_key,
                    name: failure.name,
                    error: BridgeError::from(failure.error),
                })
                .collect())
        })
        .await
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
        error @ bae_core::import::ImportListSubscriptionError::RunNotShown => {
            BridgeError::internal(error.to_string())
        }
        bae_core::import::ImportListSubscriptionError::Query(error) => {
            BridgeError::database_query(error)
        }
    }
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
mod tests;
