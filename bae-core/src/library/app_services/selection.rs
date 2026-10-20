//! Acting on the import list's selection on [`AppServices`]. The list
//! subscription owns the selection
//! ([`ImportListSubscription`](crate::import::ImportListSubscription)) and
//! hands its selected rows here, in the order the list shows them.

use super::*;
use crate::import::selection::SelectedRows;
use crate::import::triage::{keys_for, CandidateAction};
use crate::import::ImportError;

/// One selected candidate a bulk action could not run on, and why.
#[derive(Debug)]
pub struct SelectionActionFailure {
    pub candidate_key: String,
    pub name: String,
    pub error: ImportError,
}

impl AppServices {
    /// The folders of every selected candidate, for a surface to show where
    /// they are.
    pub async fn import_selection_source_folders(
        &self,
        selected: &SelectedRows,
    ) -> Result<Vec<String>, ImportError> {
        let mut folders = Vec::new();
        for candidate in &selected.candidates {
            folders.extend(
                self.import_candidate_source_folders(&candidate.candidate_key)
                    .await?,
            );
        }
        Ok(folders)
    }

    /// Read every selected candidate as one release, and answer with its key.
    pub async fn combine_import_selection(
        &self,
        selected: &SelectedRows,
    ) -> Result<String, ImportError> {
        self.import_combine_candidates(
            selected
                .candidates
                .iter()
                .map(|candidate| candidate.candidate_key.clone())
                .collect(),
        )
        .await
    }

    /// Run `action` on every selected candidate that offers it now, one at a
    /// time in the order the list shows them, telling `progress` how many are
    /// done of how many. The identification and import queues take what they
    /// are handed first to first, so the top rows start first. A candidate
    /// the action fails on is reported and the rest go on.
    pub async fn run_import_selection_action(
        &self,
        selected: &SelectedRows,
        action: CandidateAction,
        progress: impl Fn(u64, u64),
    ) -> Result<Vec<SelectionActionFailure>, ImportError> {
        let selected = &selected.candidates;
        let members = crate::import::selection::members(selected, &self.runtime_facts());
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
                .import_select_candidate_file_tags(key.to_string())
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
