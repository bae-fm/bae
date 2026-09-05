//! The pane's per-candidate state between visits, written as the person works.

use super::ImportServiceHandle;
use crate::import::{CandidateSession, MetadataPresentation, SearchForm};

impl ImportServiceHandle {
    /// Which surface the pane's metadata slot shows for this candidate.
    pub async fn set_candidate_presentation(
        &self,
        candidate_key: &str,
        presentation: MetadataPresentation,
    ) -> Result<(), crate::import::ImportError> {
        self.update_candidate_session(candidate_key, |session| {
            session.presentation = presentation;
        })
        .await
    }

    /// The typed-search form as the person left it.
    pub async fn set_candidate_search_form(
        &self,
        candidate_key: &str,
        search: SearchForm,
    ) -> Result<(), crate::import::ImportError> {
        self.update_candidate_session(candidate_key, |session| {
            session.search = search;
        })
        .await
    }

    /// The last command the pane ran for this candidate, when it failed;
    /// `None` clears the banner for the next command.
    pub async fn set_candidate_pane_error(
        &self,
        candidate_key: &str,
        error: Option<String>,
    ) -> Result<(), crate::import::ImportError> {
        self.update_candidate_session(candidate_key, |session| {
            session.error = error;
        })
        .await
    }

    /// Read the candidate's session — the stored one, or the one its pane
    /// opens on — apply `change`, and store the whole. Under the commit lock,
    /// so two writes in a row cannot lose one another's field.
    async fn update_candidate_session(
        &self,
        candidate_key: &str,
        change: impl FnOnce(&mut CandidateSession),
    ) -> Result<(), crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock().await;
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
