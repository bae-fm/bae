//! The failure the pane states for its last command, stored as the person
//! works, and whether asking for identification's results owes a run.

use super::ImportServiceHandle;
use crate::{PaneCommand, PaneFailure, PaneOutcome};

impl ImportServiceHandle {
    /// Whether showing identification's results for this candidate owes a
    /// run: none when a verdict is stored for the candidate's current files,
    /// which the pane shows as it stood, picked row and all, rather than
    /// asking again. A candidate this device imported holds no pane, so for
    /// it the answer is that its files are in the library.
    pub async fn automatic_run_owed(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<bool, crate::ImportError> {
        self.services.tables.automatic_run_owed(candidate_key).await
    }

    /// Clear the failure the pane states for its last command, as its next
    /// command starts. A key that names no stored candidate, such as a
    /// library release being identified again, has no stored pane to clear.
    pub async fn clear_pane_failure(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        let entry = self.services.tables.load_candidate_entry(candidate_key).await?;
        if !entry.is_some_and(|entry| entry.listed()) {
            return Ok(());
        }
        self.store_pane_failure(candidate_key, None).await
    }

    /// Run `run`, one of the pane's commands for this candidate: the failure
    /// the pane states for its last command is cleared first, and when this
    /// one fails its failure is stored in that place for the pane to state.
    /// An error comes back only when the pane does not state it: the failure
    /// could not be stored, or clearing the last one failed. A candidate this
    /// device imported keeps no pane, so for it the answer is that its files
    /// are in the library.
    pub async fn run_pane_command<T>(
        &self,
        candidate_key: &crate::CandidateKey,
        command: PaneCommand,
        run: impl std::future::Future<Output = Result<T, crate::ImportError>>,
    ) -> Result<PaneOutcome, crate::ImportError> {
        self.clear_pane_failure(candidate_key).await?;
        let error = match run.await {
            Ok(_) => return Ok(PaneOutcome::Done),
            Err(error) => error,
        };
        let failure = PaneFailure { command, error: error.ui_error() };
        match self.store_pane_failure(candidate_key, Some(failure)).await {
            Ok(()) => Ok(PaneOutcome::Failed),
            // Imported while the command ran: there is no pane to state its
            // failure on, and the files being in the library is the answer.
            Err(imported @ crate::ImportError::FilesAlreadyImported { .. }) => Err(imported),
            Err(store) => {
                tracing::error!(
                    "{command:?} for {candidate_key} failed, and its failure could not be stored: {store}"
                );
                Err(error)
            }
        }
    }

    /// Store `failure` as the one the pane states for the candidate's last
    /// command, or none. The write refuses a candidate this device imported,
    /// so it cannot land on one an import has committed.
    async fn store_pane_failure(
        &self,
        candidate_key: &crate::CandidateKey,
        failure: Option<PaneFailure>,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.services.tables.save_import_pane_failure(&candidate_key, failure).await
        })
        .await
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
