//! The pane's per-candidate state between visits, written as the person works.

use super::ImportServiceHandle;
use crate::{CandidateSession, PaneCommand, PaneFailure, PaneMove, PaneOutcome, SearchForm};

impl ImportServiceHandle {
    /// Move this candidate's pane by the one rule every move follows,
    /// [`CandidateSession::moved`].
    pub async fn move_candidate_pane(
        &self,
        candidate_key: &crate::CandidateKey,
        pane_move: PaneMove,
    ) -> Result<(), crate::ImportError> {
        self.update_candidate_session(candidate_key, move |session| {
            *session = session.clone().moved(pane_move);
        })
        .await
    }

    /// Open the pane on identification's results for the person, and say
    /// whether a run is owed: none when a verdict is stored for the
    /// candidate's current files, which the pane shows as it stood — picked
    /// row and all — rather than asking again.
    pub async fn open_automatic(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<bool, crate::ImportError> {
        self.move_candidate_pane(candidate_key, PaneMove::Automatic).await?;
        let stored = self.services
            .tables
            .load_import_candidate(candidate_key)
            .await?
            .is_some_and(|projection| projection.verdict.is_some());
        Ok(!stored)
    }

    /// Move the pane of every candidate whose identification was just
    /// admitted: to the page its run reports on, so a person who opens the
    /// candidate while it is being identified, or after, is on the answer.
    ///
    /// Under the commit lock like every other session write, so a
    /// read-modify-write of the rest of a session in flight cannot put the
    /// draft back over this.
    pub(crate) async fn move_admitted_panes(
        &self,
        content_hashes: Vec<String>,
    ) -> Result<(), crate::ImportError> {
        let _commit = self.services.folder_state_commit.lock("open panes on Find online").await;
        self.services.tables.move_import_candidate_panes(content_hashes, PaneMove::Admitted).await?;
        Ok(())
    }

    /// The typed-search form as the person left it.
    pub async fn set_candidate_search_form(
        &self,
        candidate_key: &crate::CandidateKey,
        search: SearchForm,
    ) -> Result<(), crate::ImportError> {
        self.update_candidate_session(candidate_key, move |session| {
            session.search = search;
        })
        .await
    }

    /// Clear the failure the pane states for its last command, as its next
    /// command starts. A key that names no stored candidate — a library
    /// release being identified again — has no stored pane to clear.
    pub async fn clear_pane_failure(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        if self.services.tables.load_import_candidate(candidate_key).await?.is_none() {
            return Ok(());
        }
        self.update_candidate_session(candidate_key, |session| session.error = None).await
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
        match self
            .update_candidate_session(candidate_key, move |session| session.error = Some(failure))
            .await
        {
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

    /// Read the candidate's session — the stored one, or the one its pane
    /// opens on — apply `change`, and store the whole. Under the commit lock,
    /// so two writes in a row cannot lose one another's field.
    async fn update_candidate_session(
        &self,
        candidate_key: &crate::CandidateKey,
        change: impl FnOnce(&mut CandidateSession) + Send + 'static,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(
            async move { this.update_candidate_session_write(&candidate_key, change).await },
        )
        .await
    }

    async fn update_candidate_session_write(
        &self,
        candidate_key: &crate::CandidateKey,
        change: impl FnOnce(&mut CandidateSession),
    ) -> Result<(), crate::ImportError> {
        let _commit = self.services.folder_state_commit.lock("store a pane session").await;
        let projection =
            self.services.tables.load_import_candidate(candidate_key).await?.ok_or_else(|| {
                crate::ImportError::Internal {
                    detail: format!("{candidate_key} is not a scanned folder candidate"),
                }
            })?;
        let content_hash = projection.candidate.files.content_hash();
        // An imported candidate holds no draft, so no pane is kept for it.
        let draft = match &projection.stage {
            crate::ProjectedStage::Drafting(draft) => draft,
            crate::ProjectedStage::Imported { release, .. } => {
                return Err(crate::ImportError::FilesAlreadyImported {
                    release_id: release.release_id.clone(),
                })
            }
        };
        let mut session = draft.session_or_initial(projection.verdict.is_some());
        change(&mut session);
        self.services.tables.save_import_candidate_session(&content_hash, &session).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
