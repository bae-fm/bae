//! The import list's selection on [`AppServices`]: changing it, and running an
//! action over it. The selection is rows in the library's database, so every
//! operation here is over those rows, whatever their number.

use super::*;
use crate::import::selection::{SelectionChange, SelectionSummary};
use crate::import::triage::{keys_for, CandidateAction};
use crate::import::{ImportError, ImportListView, MetadataProvenance};

/// One selected candidate a bulk action could not run on, and why.
#[derive(Debug)]
pub struct SelectionActionFailure {
    pub candidate_key: String,
    pub name: String,
    pub error: ImportError,
}

impl AppServices {
    /// Apply a change a person made by pointing at rows of the list `view`
    /// shows, and return the selection revision of the list read that
    /// reflects it.
    pub async fn change_import_selection(
        &self,
        view: ImportListView,
        change: SelectionChange,
    ) -> Result<u64, crate::library::LibraryError> {
        self.inner
            .manager
            .change_candidate_selection(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
                change,
            )
            .await
    }

    /// Select every candidate the list shows under `view`, loaded by a surface
    /// or not.
    pub async fn select_all_import_candidates(
        &self,
        view: ImportListView,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner
            .manager
            .select_shown_candidates(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
            )
            .await
    }

    /// Keep only the selected candidates the list shows under `view`: a view
    /// that hides a selected row takes it out of the selection, and one that
    /// shows more rows selects none of them.
    pub async fn keep_shown_import_selection(
        &self,
        view: ImportListView,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner
            .manager
            .keep_shown_candidate_selection(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
            )
            .await
    }

    /// What the selection holds and can be told to do, now and on every change
    /// to the selected rows or to what is running for them.
    pub fn subscribe_import_selection(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<SelectionSummary> {
        crate::import::selection::watch_selection(
            self.inner.manager.subscribe_selected_candidates(),
            self.inner.import.watch_runtime_facts(),
            runtime_handle,
        )
    }

    /// The folders of every selected candidate, for a surface to show where
    /// they are.
    pub async fn import_selection_source_folders(&self) -> Result<Vec<String>, ImportError> {
        let mut folders = Vec::new();
        for candidate in self.inner.manager.load_selected_candidates().await? {
            folders.extend(
                self.import_candidate_source_folders(&candidate.candidate_key)
                    .await?,
            );
        }
        Ok(folders)
    }

    /// Read every selected candidate as one release, and select that release.
    pub async fn combine_import_selection(&self) -> Result<String, ImportError> {
        let selected = self.inner.manager.load_selected_candidates().await?;
        let key = self
            .import_combine_candidates(
                selected
                    .into_iter()
                    .map(|candidate| candidate.candidate_key)
                    .collect(),
            )
            .await?;
        self.inner
            .manager
            .change_candidate_selection(
                crate::import::ImportListRequest::default(),
                SelectionChange::Replace {
                    keys: vec![key.clone()],
                },
            )
            .await?;
        Ok(key)
    }

    /// Run `action` on every selected candidate that offers it now, one at a
    /// time in the order the list shows them under `view`, telling `progress`
    /// how many are done of how many. The identification and import queues
    /// take what they are handed first to first, so the top rows start first.
    /// A candidate the action fails on is reported and the rest go on.
    pub async fn run_import_selection_action(
        &self,
        view: ImportListView,
        action: CandidateAction,
        progress: impl Fn(u64, u64),
    ) -> Result<Vec<SelectionActionFailure>, ImportError> {
        let selected = self
            .inner
            .manager
            .load_selected_candidates_in_view_order(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
            )
            .await?;
        let members = crate::import::selection::members(&selected, &self.runtime_facts());
        let keys = keys_for(&members, action);
        let total = keys.len() as u64;
        let name_of = |key: &str| {
            selected
                .iter()
                .find(|candidate| candidate.candidate_key == key)
                .map_or_else(|| key.to_string(), |candidate| candidate.name.clone())
        };
        let mut failures = Vec::new();
        progress(0, total);
        // Cancelling a batch of runs is one command, not one per candidate.
        if action == CandidateAction::CancelIdentification {
            self.cancel_identification(keys).await?;
            progress(total, total);
            return Ok(failures);
        }
        for (done, key) in keys.iter().enumerate() {
            if let Err(error) = self.run_on_candidate(action, key).await {
                tracing::error!("{action:?} failed for {key}: {error}");
                failures.push(SelectionActionFailure {
                    candidate_key: key.clone(),
                    name: name_of(key),
                    error,
                });
            }
            progress(done as u64 + 1, total);
        }
        Ok(failures)
    }

    /// Run one candidate's part of a bulk action.
    async fn run_on_candidate(
        &self,
        action: CandidateAction,
        key: &str,
    ) -> Result<(), ImportError> {
        match action {
            CandidateAction::Import => self.import_selected(key).await.map(|_| ()),
            // Re-asking what failed is identifying again: the run reads the
            // candidate afresh, and the response cache answers what had
            // succeeded.
            CandidateAction::Identify | CandidateAction::RetryIdentification => {
                self.rerun_identify(key.to_string());
                Ok(())
            }
            CandidateAction::CancelImport => self.import_cancel(key),
            CandidateAction::ResetToFileMetadata => self
                .import_select_candidate_metadata_provenance(
                    key.to_string(),
                    MetadataProvenance::FileMetadata,
                )
                .await
                .map(|_| ()),
            CandidateAction::ClearMetadata => self
                .import_clear_candidate_metadata(key.to_string())
                .await
                .map(|_| ()),
            CandidateAction::Separate => self.import_separate_candidate(key).await,
            CandidateAction::Skip => {
                self.import_set_candidate_skipped(key.to_string(), true)
                    .await
            }
            CandidateAction::Restore => {
                self.import_set_candidate_skipped(key.to_string(), false)
                    .await
            }
            // Cancelling runs, combining and showing folders act on the whole
            // selection at once.
            CandidateAction::CancelIdentification
            | CandidateAction::Combine
            | CandidateAction::RevealFolder => Err(ImportError::Internal {
                detail: format!("{action:?} does not run candidate by candidate"),
            }),
        }
    }
}
