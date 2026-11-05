//! The import tab's list as it pulls it, with the selection the list owns
//! and the bulk actions over it.

use crate::types::BridgeError;

/// The import tab's list, reconfigurable by view and by window, and the
/// selection it owns.
///
/// The same shape as [`AlbumBrowseSubscription`](crate::AlbumBrowseSubscription):
/// one object per list, `set_view` and `set_windows` reconfigure it, `next`
/// waits for the value that answers. The selection only ever holds rows the
/// list shows, so changing it and acting on it are this list's to do.
#[derive(uniffi::Object)]
pub struct ImportListSubscription {
    inner: bae_import::LiveImportList,
    services: bae_app::AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
    /// Where the next bulk action reports how far it has got: the stream
    /// [`Self::selection_action_progress`] last handed out.
    selection_progress: crate::keyed_values::ProgressStream,
}

pub(crate) fn import_list(
    inner: bae_import::LiveImportList,
    services: bae_app::AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
) -> std::sync::Arc<ImportListSubscription> {
    std::sync::Arc::new(ImportListSubscription {
        inner,
        services,
        selection_progress: crate::keyed_values::ProgressStream::new(runtime.clone()),
        runtime,
    })
}

#[uniffi::export]
impl ImportListSubscription {
    /// The stream the bulk actions over the selection report how far they
    /// have got in, from now until it is cancelled: its one key, `""`, while
    /// an action runs, and nothing once it ends. A stream handed out before
    /// it holds no later action's progress, and a cancelled one holds
    /// nothing from then on, a running action's included.
    pub fn selection_action_progress(
        &self,
    ) -> std::sync::Arc<crate::keyed_values::KeyedValueStream> {
        self.selection_progress.feed()
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ImportListSubscription {
    /// Show `view`, and return the view revision of the list value that
    /// reflects it.
    pub async fn set_view(
        self: std::sync::Arc<Self>,
        view: crate::types::BridgeImportListView,
    ) -> Result<u64, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.set_view", move || async move {
                self.inner.set_view(view.into_core()).await.map_err(list_error)
            })
            .await
    }

    /// Fold one group shut, or open it, and return the view revision of the
    /// list value that reflects it.
    pub async fn set_group_folded(
        self: std::sync::Arc<Self>,
        group: crate::types::BridgeFolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<u64, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.set_group_folded", move || async move {
                self.inner.set_group_folded(group.into_core()?, folded).await.map_err(list_error)
            })
            .await
    }

    /// Fold every group shut, or open them all, and return the view revision
    /// of the list value that reflects it.
    pub async fn set_all_groups_folded(
        self: std::sync::Arc<Self>,
        folded: bool,
    ) -> Result<u64, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.set_all_groups_folded", move || async move {
                self.inner.set_all_groups_folded(folded).await.map_err(list_error)
            })
            .await
    }

    /// The view and position that reveal the candidate at its current
    /// placement; absent when the list lists no such candidate.
    pub async fn locate_candidate(
        self: std::sync::Arc<Self>,
        candidate_key: Vec<u8>,
    ) -> Result<Option<crate::types::BridgeImportCandidateListLocation>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.locate_candidate", move || async move {
                Ok(self
                    .inner
                    .locate_candidate(&candidate_key.into())
                    .await
                    .map_err(list_error)?
                    .map(crate::types::BridgeImportCandidateListLocation::from_core))
            })
            .await
    }

    /// The first candidate still being identified, in the list's order.
    pub async fn first_identifying_candidate(
        self: std::sync::Arc<Self>,
    ) -> Result<Option<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.first_identifying_candidate", move || async move {
                Ok(self
                    .inner
                    .first_identifying_candidate()
                    .await
                    .map_err(list_error)?
                    .map(Vec::<u8>::from))
            })
            .await
    }

    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner.set_windows(super::core_windows(windows)).map_err(list_error)
    }

    /// Apply a change a person made by pointing at rows of the list, and
    /// return the selection revision of the list value that reflects it.
    pub async fn change_selection(
        self: std::sync::Arc<Self>,
        change: crate::types::BridgeSelectionChange,
    ) -> Result<u64, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.change_selection", move || async move {
                self.inner.change_selection(change.into_core()).await.map_err(list_error)
            })
            .await
    }

    /// Select every row the list shows, loaded by the surface or not, and
    /// return the selection revision of the list value that reflects it.
    pub async fn select_all(self: std::sync::Arc<Self>) -> Result<u64, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.select_all", move || async move {
                self.inner.select_all().await.map_err(list_error)
            })
            .await
    }

    /// The folders of every selected candidate, as coven's bytes for each
    /// path.
    pub async fn selection_source_folders(
        self: std::sync::Arc<Self>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.selection_source_folders", move || async move {
                Ok(self
                    .services
                    .import_selection_source_folders(
                        &self.inner.selected().await.map_err(list_error)?,
                    )
                    .await
                    .map_err(BridgeError::from)?
                    .iter()
                    .map(|folder| coven::path_bytes(folder))
                    .collect())
            })
            .await
    }

    /// Read every selected candidate as one release, select it, and answer
    /// with its key.
    pub async fn combine_selection(self: std::sync::Arc<Self>) -> Result<Vec<u8>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.combine_selection", move || async move {
                let key = self
                    .services
                    .combine_import_selection(&self.inner.selected().await.map_err(list_error)?)
                    .await
                    .map_err(BridgeError::from)?;
                self.inner
                    .change_selection(bae_import::selection::SelectionChange::Replace {
                        keys: vec![key.clone()],
                    })
                    .await
                    .map_err(list_error)?;
                Ok(key.into())
            })
            .await
    }

    /// Read the one selected release as the releases it is made of, select
    /// them, and answer with their keys.
    pub async fn separate_selection(
        self: std::sync::Arc<Self>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.separate_selection", move || async move {
                let keys = self
                    .services
                    .separate_import_selection(&self.inner.selected().await.map_err(list_error)?)
                    .await
                    .map_err(BridgeError::from)?;
                self.inner
                    .change_selection(bae_import::selection::SelectionChange::Replace {
                        keys: keys.clone(),
                    })
                    .await
                    .map_err(list_error)?;
                Ok(keys.into_iter().map(Vec::<u8>::from).collect())
            })
            .await
    }

    /// Run `action` on every selected candidate that offers it, in the order
    /// the list shows them, and answer with the candidates it failed on. How
    /// far it has got is the stream [`Self::selection_action_progress`] last
    /// handed out, which holds it while the action runs.
    pub async fn run_selection_action(
        self: std::sync::Arc<Self>,
        action: crate::types::BridgeCandidateAction,
    ) -> Result<Vec<crate::types::BridgeSelectionActionFailure>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportListSubscription.run_selection_action", move || async move {
                let progress = self.selection_progress.writer();
                let failures = self
                    .services
                    .run_import_selection_action(
                        &self.inner.selected().await.map_err(list_error)?,
                        action.into_core(),
                        |completed, total| {
                            progress.set(crate::types::BridgeFastValue::SelectionActionProgress {
                                progress: crate::types::BridgeSelectionActionProgress {
                                    completed,
                                    total,
                                },
                            });
                        },
                    )
                    .await
                    .map_err(BridgeError::from)?;
                Ok(failures
                    .into_iter()
                    .map(|failure| crate::types::BridgeSelectionActionFailure {
                        candidate_key: failure.candidate_key.into(),
                        name: failure.name,
                        error: BridgeError::from(failure.error),
                    })
                    .collect())
            })
            .await
    }
}

pulled_read_calls! {
    ImportListSubscription => crate::types::BridgeImportListSnapshot,
    converted by crate::types::BridgeImportListSnapshot::from_core
}

impl super::PulledRead for bae_import::LiveImportList {
    type Value = bae_import::ImportListSnapshot;

    async fn next(&self) -> Result<bae_import::ImportListSnapshot, BridgeError> {
        bae_import::LiveImportList::next(self).await.map_err(list_error)
    }

    async fn cancel(&self) -> Result<(), BridgeError> {
        bae_import::LiveImportList::cancel(self).await.map_err(list_error)
    }
}

fn list_error(error: bae_import::LiveImportListError) -> BridgeError {
    match error {
        bae_import::LiveImportListError::Cancelled => BridgeError::Cancelled,
        error @ bae_import::LiveImportListError::RunNotShown => {
            BridgeError::internal(error.to_string())
        }
        bae_import::LiveImportListError::Query(error) => BridgeError::database_query(error),
        bae_import::LiveImportListError::Database(error) => BridgeError::from(error),
    }
}
