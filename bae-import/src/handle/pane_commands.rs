//! The commands the import pane runs for its candidate that only the import
//! service takes part in. Each starts by clearing the failure the pane states
//! for its last command, and a failure of its own is stored with the candidate
//! for the pane to state, so the pane shows what core stored rather than what
//! a surface made of an error. The commands that also touch the
//! identification queue are on [`crate::IdentificationHandle`].

use super::ImportServiceHandle;
use crate::{ImportError, PaneCommand, PaneMove, PaneOutcome};
use bae_catalogs::SearchQuery;
use bae_identify::PressingLink;

impl ImportServiceHandle {
    /// Import the candidate the pane shows.
    pub async fn pane_start_import(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<PaneOutcome, ImportError> {
        self.run_pane_command(candidate_key, PaneCommand::Import, self.start_import(candidate_key))
            .await
    }

    /// Link the candidate to the release `link` names and read its draft from
    /// it, each fetch it waits on told to `steps`. A catalog release that
    /// fails to load, or that lists another number of tracks than the folder
    /// holds, is told on that release's own row, so its failure comes back as
    /// the error.
    pub async fn pane_select_release(
        &self,
        candidate_key: crate::CandidateKey,
        link: PressingLink,
        steps: crate::CatalogApplySteps,
    ) -> Result<PaneOutcome, ImportError> {
        self.clear_pane_failure(&candidate_key).await?;
        self.select_candidate_release(candidate_key.clone(), link, steps).await?;
        // A pick that landed leaves the draft it read to see.
        self.move_candidate_pane(&candidate_key, PaneMove::Picked).await?;
        Ok(PaneOutcome::Done)
    }

    /// Read the candidate's draft from its files' own tags, a pane command
    /// whose failure the pane states. The release link stays as it is.
    pub async fn pane_read_file_tags(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<PaneOutcome, ImportError> {
        let outcome = self
            .run_pane_command(
                &candidate_key,
                PaneCommand::ReadFileTags,
                self.select_candidate_file_tags(candidate_key.clone()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            self.move_candidate_pane(&candidate_key, PaneMove::Picked).await?;
        }
        Ok(outcome)
    }

    /// Unlink the candidate from its release, leaving its draft as it is.
    pub async fn pane_unlink_release(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<PaneOutcome, ImportError> {
        self.run_pane_command(
            candidate_key,
            PaneCommand::Unlink,
            self.unlink_candidate_release(candidate_key.clone()),
        )
        .await
    }

    /// Keep the candidate's own draft over what its lookup offered, and go
    /// back to the draft.
    pub async fn pane_keep_own_draft(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<PaneOutcome, ImportError> {
        let outcome = self
            .run_pane_command(
                candidate_key,
                PaneCommand::KeepOwnDraft,
                self.keep_candidate_draft(candidate_key.clone()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            self.move_candidate_pane(candidate_key, PaneMove::Picked).await?;
        }
        Ok(outcome)
    }

    /// Link the candidate to the album the pressings its lookup offers are
    /// of, its pressing unknown, taking what they agree on into its draft,
    /// and go back to the draft.
    pub async fn pane_link_shared_album(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<PaneOutcome, ImportError> {
        let outcome = self
            .run_pane_command(
                candidate_key,
                PaneCommand::LinkSharedAlbum,
                self.link_candidate_shared_album(candidate_key.clone()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            self.move_candidate_pane(candidate_key, PaneMove::Picked).await?;
        }
        Ok(outcome)
    }

    /// Submit the candidate's typed search. What it turns up, failures
    /// included, lands on the candidate's runtime rather than the pane's
    /// banner, so this only clears the banner for it.
    pub async fn pane_start_candidate_search(
        &self,
        candidate_key: crate::CandidateKey,
        query: SearchQuery,
    ) -> Result<(), ImportError> {
        self.clear_pane_failure(&candidate_key).await?;
        self.start_candidate_search(candidate_key, query).await
    }
}
