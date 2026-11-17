//! The import tab's list as the app holds it open: its view and folds, the
//! bulk actions over the rows the app keeps selected, and the live reads of
//! its windows, count, position, its summary and what the selected rows hold.

use crate::types::{BridgeError, BridgeSortValue};

/// The import tab's open list. Its windows, count and position are the same
/// live reads every list has (`subscribe_list_window`, `subscribe_list_count`,
/// `subscribe_list_position` and `list_key_at`), opened from the list because
/// they read the list's own view and folds; the list opens a separate live
/// read for its summary, Found's filter entries and what the selected rows
/// hold. When the app cancels the list, every read opened from the list ends.
/// The app keeps the keys of the rows the person selected, and passes them to
/// each call that reads or acts on them; each action acts on the ones the
/// list shows.
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

    /// Read the rows of the list in `window`, in the list's order, and keep
    /// reading them live.
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

    /// What the rows the app keeps selected hold and the actions they offer,
    /// whatever the list shows, read live. The read starts with no row, and
    /// `set_selected` names the rows.
    pub fn subscribe_selection(&self) -> std::sync::Arc<ImportSelectionSubscription> {
        import_selection(bae_app::import_list_selection(&self.inner), self.runtime.clone())
    }
}

pulled_read! {
    /// The import list's watched folders, whether Found has groups, and what
    /// the view narrows by.
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
    /// What the rows the app keeps selected in the import list hold and the
    /// actions they offer. `set_selected` changes the rows without opening a
    /// new read.
    ImportSelectionSubscription:
        bae_app::live_reads::LiveImportSelection => crate::types::BridgeSelectionSummary,
    converted by crate::types::BridgeSelectionSummary::from_core,
    built by import_selection
}

#[uniffi::export]
impl ImportSelectionSubscription {
    /// Read what the rows at `selected` hold from now on.
    pub fn set_selected(&self, selected: Vec<Vec<u8>>) -> Result<(), BridgeError> {
        self.inner
            .set(keys(selected).into_iter().collect())
            .map(|_revision| ())
            .map_err(BridgeError::from)
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ImportList {
    /// Show `view`. `selected` holds the keys the app keeps selected. On
    /// another tab, filter or search, the answer says where the new view
    /// opens, at the first selected row the view shows, by key and position,
    /// or at its top, and lists the selected keys the new view doesn't show,
    /// which leave the selection. Another order, and the view the list
    /// already shows, answer nothing. The app drops the windows it holds,
    /// opens new ones from the place the answer names, and scrolls the table
    /// there.
    pub async fn set_view(
        self: std::sync::Arc<Self>,
        view: crate::types::BridgeImportListView,
        selected: Vec<Vec<u8>>,
    ) -> Result<Option<crate::types::BridgeImportListViewChange>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_view", move || async move {
                Ok(self
                    .inner
                    .set_view(view.into_core(), keys(selected))
                    .await
                    .map_err(list_error)?
                    .map(crate::types::BridgeImportListViewChange::from_core))
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

    /// Close the list: every read opened from it ends, and its folds go.
    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.cancel", move || async move {
                self.inner.cancel().await.map_err(list_error)
            })
            .await
    }

    /// Fold one group shut, or open it, and answer with the keys of
    /// `selected`, the rows the app keeps selected, whose rows the fold hid.
    pub async fn set_group_folded(
        self: std::sync::Arc<Self>,
        group: crate::types::BridgeFolderReleaseDecisionKey,
        folded: bool,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_group_folded", move || async move {
                Ok(bytes(
                    self.inner
                        .set_group_folded(group.into_core()?, folded, keys(selected))
                        .await
                        .map_err(list_error)?,
                ))
            })
            .await
    }

    /// Fold every group shut, or open them all, and answer with the keys of
    /// `selected`, the rows the app keeps selected, whose rows the fold hid.
    pub async fn set_all_groups_folded(
        self: std::sync::Arc<Self>,
        folded: bool,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.set_all_groups_folded", move || async move {
                Ok(bytes(
                    self.inner
                        .set_all_groups_folded(folded, keys(selected))
                        .await
                        .map_err(list_error)?,
                ))
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

    /// Where the candidate sits in what the list shows under its view now,
    /// its filters and folds as they are; absent when the view does not
    /// show it.
    pub async fn candidate_position(
        self: std::sync::Arc<Self>,
        candidate_key: Vec<u8>,
    ) -> Result<Option<u64>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.candidate_position", move || async move {
                self.inner.candidate_position(&candidate_key.into()).await.map_err(list_error)
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

    /// The keys of every row the list shows, loaded by the app or not: what
    /// Select All selects.
    pub async fn keys(self: std::sync::Arc<Self>) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.keys", move || async move {
                Ok(bytes(self.inner.keys().await.map_err(list_error)?))
            })
            .await
    }

    /// The keys of the rows the list shows from the row `from` to the row
    /// `to`, both included, in the list's order: what Shift-clicking a row
    /// selects. The app passes the tab and order it shows the list in as
    /// `tab` and `order`. When the list shows only one of `from` and `to`,
    /// the answer holds that row's key alone. When the list shows neither,
    /// or shows another tab, the answer holds no keys.
    pub async fn keys_between(
        self: std::sync::Arc<Self>,
        tab: crate::types::BridgeTriageTab,
        order: crate::types::BridgeImportListOrder,
        from: Vec<u8>,
        to: Vec<u8>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.keys_between", move || async move {
                Ok(bytes(
                    self.inner
                        .keys_between(tab.into_core(), order.into_core(), from.into(), to.into())
                        .await
                        .map_err(list_error)?,
                ))
            })
            .await
    }

    /// The folders of every candidate at `selected` the list shows, as
    /// coven's bytes for each path.
    pub async fn selection_source_folders(
        self: std::sync::Arc<Self>,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.selection_source_folders", move || async move {
                Ok(self
                    .services
                    .import_selection_source_folders(
                        &self.inner.selected(keys(selected)).await.map_err(list_error)?,
                    )
                    .await
                    .map_err(BridgeError::from)?
                    .iter()
                    .map(|folder| coven::path_bytes(folder))
                    .collect())
            })
            .await
    }

    /// Read every candidate at `selected` the list shows as one release, and
    /// answer with its key, which the app selects.
    pub async fn combine_selection(
        self: std::sync::Arc<Self>,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.combine_selection", move || async move {
                Ok(self
                    .services
                    .combine_import_selection(
                        &self.inner.selected(keys(selected)).await.map_err(list_error)?,
                    )
                    .await
                    .map_err(BridgeError::from)?
                    .into())
            })
            .await
    }

    /// Read the one release at `selected` as the releases it is made of, and
    /// answer with their keys, which the app selects.
    pub async fn separate_selection(
        self: std::sync::Arc<Self>,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.separate_selection", move || async move {
                Ok(bytes(
                    self.services
                        .separate_import_selection(
                            &self.inner.selected(keys(selected)).await.map_err(list_error)?,
                        )
                        .await
                        .map_err(BridgeError::from)?,
                ))
            })
            .await
    }

    /// Run `action` on every candidate at `selected` the list shows that
    /// offers it, in the order the list shows them, and answer with the
    /// candidates it failed on. How far it has got is the stream
    /// [`Self::selection_action_progress`] last handed out, which holds it
    /// while the action runs.
    pub async fn run_selection_action(
        self: std::sync::Arc<Self>,
        action: crate::types::BridgeCandidateAction,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<crate::types::BridgeSelectionActionFailure>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.run_selection_action", move || async move {
                let progress = self.selection_progress.writer();
                let failures = self
                    .services
                    .run_import_selection_action(
                        &self
                            .inner
                            .selected_offering(keys(selected), action.into_core())
                            .await
                            .map_err(list_error)?,
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
        bae_import::ImportListFailure::Database(error) => BridgeError::from(error),
    }
}

/// Candidate keys as the app sends them.
fn keys(keys: Vec<Vec<u8>>) -> Vec<bae_import::CandidateKey> {
    keys.into_iter().map(bae_import::CandidateKey::from).collect()
}

/// Candidate keys as the app reads them.
fn bytes(keys: Vec<bae_import::CandidateKey>) -> Vec<Vec<u8>> {
    keys.into_iter().map(Vec::<u8>::from).collect()
}
