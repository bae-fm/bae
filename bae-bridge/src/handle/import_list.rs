use super::*;
use std::collections::BTreeSet;

forward! { async this => {
    /// Apply a change a person made by pointing at rows of the list `view`
    /// shows, and return the selection revision of the list value that
    /// reflects it.
    fn change_import_selection(
        view: crate::types::BridgeImportListView,
        change: crate::types::BridgeSelectionChange,
    ) -> u64 {
        this.services
            .change_import_selection(view.into_core(), change.into_core())
            .await
            .map_err(BridgeError::database_query)
    }

    /// Select every candidate the list shows under `view`, loaded by the
    /// surface or not.
    fn select_all_import_candidates(view: crate::types::BridgeImportListView) -> () {
        this.services
            .select_all_import_candidates(view.into_core())
            .await
            .map_err(BridgeError::database_query)
    }

    /// Keep only the selected candidates the list shows under `view`.
    fn keep_shown_import_selection(view: crate::types::BridgeImportListView) -> () {
        this.services
            .keep_shown_import_selection(view.into_core())
            .await
            .map_err(BridgeError::database_query)
    }

    /// The folders of every selected candidate.
    fn import_selection_source_folders() -> Vec<String> {
        this.services
            .import_selection_source_folders()
            .await
            .map_err(BridgeError::from)
    }

    /// Read every selected candidate as one release, select it, and answer
    /// with its key.
    fn combine_import_selection() -> String {
        this.services
            .combine_import_selection()
            .await
            .map_err(BridgeError::from)
    }

    /// Run `action` on every selected candidate that offers it, reporting how
    /// far it has got, and answer with the candidates it failed on.
    fn run_import_selection_action(
        action: crate::types::BridgeCandidateAction,
        progress: Box<dyn crate::types::SelectionActionProgressCallback>,
    ) -> Vec<crate::types::BridgeSelectionActionFailure> {
        let failures = this
            .services
            .run_import_selection_action(action.into_core(), |completed, total| {
                progress.on_progress(crate::types::BridgeSelectionActionProgress {
                    completed,
                    total,
                })
            })
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
    }

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

    fn merge_candidate_artist_identity_conflict(
        candidate_key: String,
        surviving_artist_id: String,
    ) -> () {
        this.services
            .import_merge_candidate_artist_identity_conflict(&candidate_key, &surviving_artist_id)
            .await
            .map_err(BridgeError::import)
    }
} }

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
        let runtime = self.runtime.clone();
        std::sync::Arc::new(ImportListSubscription {
            inner: self
                .services
                .subscribe_import_list(view.into_core(), &runtime),
            runtime,
        })
    }

    /// What the import list's selection holds and can be told to do, now and
    /// on every change.
    pub fn subscribe_import_selection(
        &self,
        callback: Box<dyn crate::types::ImportSelectionCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, runtime| services.subscribe_import_selection(runtime),
            move |value| {
                callback.on_value(crate::types::BridgeSelectionSummary {
                    count: value.count,
                    single: value.single,
                    offers: value
                        .offers
                        .into_iter()
                        .map(crate::types::BridgeSelectionOffer::from_core)
                        .collect(),
                })
            },
        )
    }

    /// What is running for one candidate and the commands its row offers with
    /// it, now and on every change. `basis` is the row's own; a row delivered
    /// again with a different one subscribes again.
    pub fn subscribe_candidate_live_state(
        &self,
        candidate_key: String,
        basis: crate::types::BridgeCandidateActionBasis,
        callback: Box<dyn crate::types::CandidateLiveStateCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, _| {
                services.subscribe_candidate_live_state(candidate_key, basis.into_core())
            },
            move |value| {
                callback.on_value(crate::types::BridgeCandidateLiveState::from_core(value))
            },
        )
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
        self.live_subscription(move |services, _| async move {
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
