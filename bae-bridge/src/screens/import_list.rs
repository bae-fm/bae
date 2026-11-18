//! The import tab's list as the app reads it: the live reads of its rows,
//! count, summary and what the selected rows hold, each taking the view and
//! folds the app shows in its request, and the bulk actions over the rows the
//! app keeps selected.

use crate::types::BridgeError;

/// The import tab's list. Its view and count are the same live reads every
/// list has (`subscribe_list_view` and `subscribe_list_count`), here taking
/// the request the app shows; the list opens a separate live read for its
/// summary, Found's filter entries and what the selected rows hold. The app
/// keeps the view, the folded groups and the keys of the rows the person
/// selected, and passes the request and the keys to each call that reads or
/// acts on them; each action acts on the ones the list shows under the
/// request.
#[derive(uniffi::Object)]
pub struct ImportList {
    services: bae_app::AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
    /// Where the next bulk action reports how far it has got: the stream
    /// [`Self::selection_action_progress`] last handed out.
    selection_progress: crate::keyed_values::ProgressStream,
}

pub(crate) fn import_list(
    services: bae_app::AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
) -> std::sync::Arc<ImportList> {
    std::sync::Arc::new(ImportList {
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

    /// Read the list `request` shows through its view, which keeps one live
    /// window of the list around the rows from position `first` to position
    /// `last` on screen, and moves it as the screen reports other rows.
    pub fn subscribe_list_view(
        &self,
        request: crate::types::BridgeImportListRequest,
        first: u64,
        last: u64,
    ) -> Result<std::sync::Arc<crate::ListViewSubscription>, BridgeError> {
        Ok(crate::screens::list_view(
            self.services.open_list_view(
                self.services.import_list_reads(request.into_core()?),
                bae_views::list_view::Visible::new(first, last),
            ),
            self.runtime.clone(),
        ))
    }

    /// How many items the list `request` shows holds and how many its
    /// narrowing keeps, read live.
    pub fn subscribe_list_count(
        &self,
        request: crate::types::BridgeImportListRequest,
    ) -> Result<std::sync::Arc<crate::ListCountSubscription>, BridgeError> {
        Ok(crate::screens::list_count(
            self.services.import_list_count(request.into_core()?),
            self.runtime.clone(),
        ))
    }

    /// The watched folders and whether Found has groups, read live.
    pub fn subscribe_summary(&self) -> std::sync::Arc<ImportSummarySubscription> {
        import_summary(self.services.import_list_summary(), self.runtime.clone())
    }

    /// Every entry of Found's filter and how many of Found's rows it holds,
    /// read live.
    pub fn subscribe_filter_entries(&self) -> std::sync::Arc<ImportFilterEntriesSubscription> {
        import_filter_entries(self.services.import_list_filter_entries(), self.runtime.clone())
    }

    /// What the rows the app keeps selected hold and the actions they offer,
    /// whatever the list shows, read live. The read starts with no row, and
    /// `set_selected` names the rows.
    pub fn subscribe_selection(&self) -> std::sync::Arc<ImportSelectionSubscription> {
        import_selection(self.services.import_list_selection(), self.runtime.clone())
    }
}

pulled_read! {
    /// The import list's watched folders and whether Found has groups.
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
    /// Where the list `request` shows opens, at the position of the first
    /// selected row it shows, or at its top, and the selected keys it doesn't
    /// show, which leave the selection. The app asks for another tab, filter
    /// or search with this, opens a new view under the request at the place
    /// it returns, and scrolls the table there.
    pub async fn view_change(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
    ) -> Result<crate::types::BridgeImportListViewChange, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.view_change", move || async move {
                Ok(crate::types::BridgeImportListViewChange::from_core(
                    self.services
                        .import_list_view_change(request.into_core()?, keys(selected))
                        .await?,
                ))
            })
            .await
    }

    /// The keys of `selected`, the rows the app keeps selected, that the
    /// list showed under `before` and doesn't show under `after`: what a
    /// fold hid. The two requests differ in their folds only.
    pub async fn hidden_by_fold(
        self: std::sync::Arc<Self>,
        before: crate::types::BridgeImportListRequest,
        after: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.hidden_by_fold", move || async move {
                Ok(bytes(
                    self.services
                        .import_list_hidden_by(
                            before.into_core()?,
                            after.into_core()?,
                            keys(selected),
                        )
                        .await?,
                ))
            })
            .await
    }

    /// The key of every group header Found holds: what folding every group
    /// folds.
    pub async fn group_keys(
        self: std::sync::Arc<Self>,
    ) -> Result<Vec<crate::types::BridgeFolderReleaseDecisionKey>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.group_keys", move || async move {
                Ok(self
                    .services
                    .import_group_keys()
                    .await?
                    .into_iter()
                    .map(crate::types::BridgeFolderReleaseDecisionKey::from_core)
                    .collect())
            })
            .await
    }

    /// The view that reveals the candidate at its current placement: its tab,
    /// with the filters cleared and its group opened; absent when no tab
    /// lists such a candidate.
    pub async fn locate_candidate(
        self: std::sync::Arc<Self>,
        candidate_key: Vec<u8>,
    ) -> Result<Option<crate::types::BridgeImportCandidateListLocation>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.locate_candidate", move || async move {
                Ok(self
                    .services
                    .import_locate_candidate(&candidate_key.into())
                    .await?
                    .map(crate::types::BridgeImportCandidateListLocation::from_core))
            })
            .await
    }

    /// Where the candidate sits in what the list `request` shows; absent when
    /// the list doesn't show it.
    pub async fn candidate_position(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        candidate_key: Vec<u8>,
    ) -> Result<Option<u64>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.candidate_position", move || async move {
                Ok(self
                    .services
                    .import_candidate_position(request.into_core()?, &candidate_key.into())
                    .await?)
            })
            .await
    }

    /// The first candidate still being identified, in the order `order` runs.
    pub async fn first_identifying_candidate(
        self: std::sync::Arc<Self>,
        order: crate::types::BridgeImportListOrder,
    ) -> Result<Option<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.first_identifying_candidate", move || async move {
                Ok(self
                    .services
                    .import_first_identifying_candidate(order.into_core())
                    .await?
                    .map(Vec::<u8>::from))
            })
            .await
    }

    /// The keys of every row the list `request` shows, loaded by the app or
    /// not: what Select All selects.
    pub async fn keys(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.keys", move || async move {
                Ok(bytes(self.services.import_list_keys(request.into_core()?).await?))
            })
            .await
    }

    /// The keys of the rows the list `request` shows from the row `from` to
    /// the row `to`, both included, in the list's order: what Shift-clicking
    /// a row selects. When the list shows only one of `from` and `to`, the
    /// answer holds that row's key alone. When the list shows neither, the
    /// answer holds no keys.
    pub async fn keys_between(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        from: Vec<u8>,
        to: Vec<u8>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.keys_between", move || async move {
                Ok(bytes(
                    self.services
                        .import_list_keys_between(request.into_core()?, from.into(), to.into())
                        .await?,
                ))
            })
            .await
    }

    /// The folders of every candidate at `selected` the list `request` shows,
    /// as coven's bytes for each path.
    pub async fn selection_source_folders(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.selection_source_folders", move || async move {
                Ok(self
                    .services
                    .import_selection_source_folders(&self.selected(request, selected, None).await?)
                    .await
                    .map_err(BridgeError::from)?
                    .iter()
                    .map(|folder| coven::path_bytes(folder))
                    .collect())
            })
            .await
    }

    /// Read every candidate at `selected` the list `request` shows as one
    /// release, and answer with its key, which the app selects.
    pub async fn combine_selection(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.combine_selection", move || async move {
                Ok(self
                    .services
                    .combine_import_selection(&self.selected(request, selected, None).await?)
                    .await
                    .map_err(BridgeError::from)?
                    .into())
            })
            .await
    }

    /// Read the one release at `selected` the list `request` shows as the
    /// releases it is made of, and answer with their keys, which the app
    /// selects.
    pub async fn separate_selection(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ImportList.separate_selection", move || async move {
                Ok(bytes(
                    self.services
                        .separate_import_selection(&self.selected(request, selected, None).await?)
                        .await
                        .map_err(BridgeError::from)?,
                ))
            })
            .await
    }

    /// Run `action` on every candidate at `selected` the list `request` shows
    /// that offers it, in the order the list shows them, and answer with the
    /// candidates it failed on. How far it has got is the stream
    /// [`Self::selection_action_progress`] last handed out, which holds it
    /// while the action runs.
    pub async fn run_selection_action(
        self: std::sync::Arc<Self>,
        request: crate::types::BridgeImportListRequest,
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
                        &self.selected(request, selected, Some(action.into_core())).await?,
                        action.into_core(),
                        |completed, total| {
                            progress.set(crate::types::BridgeFastValue::SelectionActionProgress {
                                progress: crate::types::BridgeSelectionActionProgress::new(
                                    completed, total,
                                ),
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

impl ImportList {
    /// The rows at `selected` the list `request` shows, in its order; when
    /// `acting` names a command, only the rows that offer it.
    async fn selected(
        &self,
        request: crate::types::BridgeImportListRequest,
        selected: Vec<Vec<u8>>,
        acting: Option<bae_import::CandidateAction>,
    ) -> Result<bae_import::selection::SelectedRows, BridgeError> {
        Ok(self.services.import_list_selected(request.into_core()?, keys(selected), acting).await?)
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
