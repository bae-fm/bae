//! The import tab's list as the app holds it open: its view and folds, the
//! selection it owns and the bulk actions over it, and the live reads of its
//! windows, count, position and chrome.

use crate::types::{BridgeError, BridgeSortValue};

/// The import tab's open list. Its windows, count and position are the same
/// live reads every list has (`subscribe_list_window`, `subscribe_list_count`,
/// `subscribe_list_position` and `list_key_at`), opened from the list because
/// they read the list's own view, folds and selection; the chrome around
/// them, Found's filter entries and the selection are reads of their own.
/// Every read opened from it ends when it is cancelled. The selection only
/// ever holds rows the list shows, so changing it and acting on it are this
/// list's to do.
#[derive(uniffi::Object)]
pub struct ImportList {
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
) -> std::sync::Arc<ImportList> {
    std::sync::Arc::new(ImportList {
        inner,
        services,
        selection_progress: crate::keyed_values::ProgressStream::new(runtime.clone()),
        runtime,
    })
}

#[uniffi::export]
impl ImportList {
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

    /// `window` of the list, a range of its order, read live from now on.
    pub fn subscribe_list_window(
        &self,
        window: crate::types::BridgeListWindow,
    ) -> std::sync::Arc<crate::ListWindowSubscription> {
        crate::screens::list_window(
            bae_app::import_list_window(&self.inner, window.into_core()),
            self.runtime.clone(),
        )
    }

    /// How many items the list holds and how many its narrowing keeps, read
    /// live.
    pub fn subscribe_list_count(&self) -> std::sync::Arc<crate::ListCountSubscription> {
        crate::screens::list_count(bae_app::import_list_count(&self.inner), self.runtime.clone())
    }

    /// How many items come before the one at `key`, read live.
    pub fn subscribe_list_position(
        &self,
        key: Vec<BridgeSortValue>,
    ) -> std::sync::Arc<crate::ListPositionSubscription> {
        crate::screens::list_position(
            bae_app::import_list_position(&self.inner, crate::screens::order_key(key)),
            self.runtime.clone(),
        )
    }

    /// The watched folders, whether Found has groups and what the view
    /// narrows by, read live.
    pub fn subscribe_summary(&self) -> std::sync::Arc<ImportSummarySubscription> {
        import_summary(bae_app::import_list_summary(&self.inner), self.runtime.clone())
    }

    /// Every entry of Found's filter and how many of Found's rows it holds,
    /// read live.
    pub fn subscribe_filter_entries(&self) -> std::sync::Arc<ImportFilterEntriesSubscription> {
        import_filter_entries(
            bae_app::import_list_filter_entries(&self.inner),
            self.runtime.clone(),
        )
    }

    /// What the selection holds and can be told to do, read live.
    pub fn subscribe_selection(&self) -> std::sync::Arc<ImportSelectionSubscription> {
        import_selection(bae_app::import_list_selection(&self.inner), self.runtime.clone())
    }
}

pulled_read! {
    /// The chrome around the import list: its watched folders, whether Found
    /// has groups, and what the view narrows by.
    ImportSummarySubscription:
        bae_store::live_query::LiveValues<bae_import::ImportQueueSummary>
            => crate::types::BridgeImportQueueSummary,
    converted by crate::types::BridgeImportQueueSummary::from_core,
    built by import_summary
}

pulled_read! {
    /// Every entry of Found's filter, with how many of Found's rows it holds.
    ImportFilterEntriesSubscription:
        bae_store::live_query::LiveValues<Vec<bae_import::PendingFilterEntry>>
            => Vec<crate::types::BridgePendingFilterEntry>,
    converted by |entries: Vec<bae_import::PendingFilterEntry>| {
        entries.into_iter().map(crate::types::BridgePendingFilterEntry::from_core).collect::<Vec<_>>()
    },
    built by import_filter_entries
}

pulled_read! {
    /// What the import list's selection holds and can be told to do.
    ImportSelectionSubscription:
        bae_store::live_query::LiveValues<bae_import::selection::SelectionSummary>
            => crate::types::BridgeSelectionSummary,
    converted by crate::types::BridgeSelectionSummary::from_core,
    built by import_selection
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ImportList {
    /// Show `view`, and answer where it opens: its first selected row, by
    /// key and position, or its top. Nothing when it is the view the list
    /// already shows. The app drops the windows it holds and opens new ones
    /// from that place.
    pub async fn set_view(
        self: std::sync::Arc<Self>,
        view: crate::types::BridgeImportListView,
    ) -> Result<Option<crate::types::BridgeImportListOpening>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_view", move || async move {
                Ok(self
                    .inner
                    .set_view(view.into_core())
                    .await
                    .map_err(list_error)?
                    .map(crate::types::BridgeImportListOpening::from_core))
            })
            .await
    }

    /// The key of the item at `position`, or of the last item when the list
    /// ends before it; none for a list with no items. A jump reads it once
    /// and starts its windows there.
    pub async fn list_key_at(
        self: std::sync::Arc<Self>,
        position: u64,
    ) -> Result<Option<Vec<BridgeSortValue>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.list_key_at", move || async move {
                Ok(self
                    .inner
                    .key_at(position)
                    .await
                    .map_err(list_error)?
                    .as_ref()
                    .map(BridgeSortValue::of_key))
            })
            .await
    }

    /// Close the list: every read opened from it ends, and its folds and
    /// selection go.
    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.cancel", move || async move {
                self.inner.cancel().await.map_err(list_error)
            })
            .await
    }

    /// Fold one group shut, or open it.
    pub async fn set_group_folded(
        self: std::sync::Arc<Self>,
        group: crate::types::BridgeFolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_group_folded", move || async move {
                self.inner.set_group_folded(group.into_core()?, folded).await.map_err(list_error)
            })
            .await
    }

    /// Fold every group shut, or open them all.
    pub async fn set_all_groups_folded(
        self: std::sync::Arc<Self>,
        folded: bool,
    ) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_all_groups_folded", move || async move {
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
            .run("ImportList.locate_candidate", move || async move {
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
            .run("ImportList.first_identifying_candidate", move || async move {
                Ok(self
                    .inner
                    .first_identifying_candidate()
                    .await
                    .map_err(list_error)?
                    .map(Vec::<u8>::from))
            })
            .await
    }

    /// Apply a change a person made by pointing at rows of the list. The
    /// windows holding the rows it changed show it.
    pub async fn change_selection(
        self: std::sync::Arc<Self>,
        change: crate::types::BridgeSelectionChange,
    ) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.change_selection", move || async move {
                self.inner.change_selection(change.into_core()).await.map_err(list_error)
            })
            .await
    }

    /// Select every row the list shows, loaded by the surface or not.
    pub async fn select_all(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.select_all", move || async move {
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
            .run("ImportList.selection_source_folders", move || async move {
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
            .run("ImportList.combine_selection", move || async move {
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
            .run("ImportList.separate_selection", move || async move {
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
            .run("ImportList.run_selection_action", move || async move {
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

fn list_error(error: bae_import::ImportListFailure) -> BridgeError {
    match error {
        bae_import::ImportListFailure::Closed => BridgeError::Cancelled,
        error @ bae_import::ImportListFailure::RunNotShown => {
            BridgeError::internal(error.to_string())
        }
        bae_import::ImportListFailure::Database(error) => BridgeError::from(error),
    }
}
