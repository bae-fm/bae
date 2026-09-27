//! The pane's per-candidate state between visits, written as the person works.

use super::ImportServiceHandle;
use crate::import::{
    CandidateSession, MetadataPresentation, PaneCommand, PaneFailure, PaneOutcome, SearchForm,
};

impl ImportServiceHandle {
    /// Which surface the pane's metadata slot shows for this candidate.
    pub async fn set_candidate_presentation(
        &self,
        candidate_key: &str,
        presentation: MetadataPresentation,
    ) -> Result<(), crate::import::ImportError> {
        self.update_candidate_session(candidate_key, move |session| {
            session.presentation = presentation;
        })
        .await
    }

    /// Open the pane on Find online for every candidate whose identification
    /// was just admitted — the page its run reports on, so a person who opens
    /// the candidate while it is being identified, or after, is on the answer.
    ///
    /// Under the commit lock like every other session write, so a
    /// read-modify-write of the rest of a session in flight cannot put the
    /// draft back over this.
    pub(crate) async fn open_find_online_for_admitted(
        &self,
        content_hashes: Vec<String>,
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock("open panes on Find online").await;
        self.library_manager
            .open_import_candidate_sessions_on_find_online(content_hashes)
            .await?;
        Ok(())
    }

    /// The typed-search form as the person left it.
    pub async fn set_candidate_search_form(
        &self,
        candidate_key: &str,
        search: SearchForm,
    ) -> Result<(), crate::import::ImportError> {
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
        candidate_key: &str,
    ) -> Result<(), crate::import::ImportError> {
        if self
            .library_manager
            .load_import_candidate(candidate_key)
            .await?
            .is_none()
        {
            return Ok(());
        }
        self.update_candidate_session(candidate_key, |session| session.error = None)
            .await
    }

    /// Run `run`, one of the pane's commands for this candidate: the failure
    /// the pane states for its last command is cleared first, and when this
    /// one fails its failure is stored in that place for the pane to state.
    /// An error comes back only when the pane does not state it — the failure
    /// could not be stored, or clearing the last one failed.
    pub async fn run_pane_command<T>(
        &self,
        candidate_key: &str,
        command: PaneCommand,
        run: impl std::future::Future<Output = Result<T, crate::import::ImportError>>,
    ) -> Result<PaneOutcome, crate::import::ImportError> {
        self.clear_pane_failure(candidate_key).await?;
        let error = match run.await {
            Ok(_) => return Ok(PaneOutcome::Done),
            Err(error) => error,
        };
        let failure = PaneFailure {
            command,
            error: error.ui_error(),
        };
        match self
            .update_candidate_session(candidate_key, move |session| {
                session.error = Some(failure)
            })
            .await
        {
            Ok(()) => Ok(PaneOutcome::Failed),
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
        candidate_key: &str,
        change: impl FnOnce(&mut CandidateSession) + Send + 'static,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.update_candidate_session_write(&candidate_key, change)
                .await
        })
        .await
    }

    async fn update_candidate_session_write(
        &self,
        candidate_key: &str,
        change: impl FnOnce(&mut CandidateSession),
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock("store a pane session").await;
        let projection = self
            .library_manager
            .load_import_candidate(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            })?;
        let content_hash = projection.candidate.files.content_hash();
        let mut session = projection.session_or_initial();
        change(&mut session);
        self.library_manager
            .save_import_candidate_session(&content_hash, &session)
            .await?;
        Ok(())
    }
}
