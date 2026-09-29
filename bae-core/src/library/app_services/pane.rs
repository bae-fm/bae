//! The commands the import pane runs for its candidate. Each starts by
//! clearing the failure the pane states for its last command, and a failure of
//! its own is stored with the candidate for the pane to state, so the pane
//! shows what core stored rather than what a surface made of an error.

use super::*;
use crate::import::{
    ImportError, LookupChoiceEdit, PaneCommand, PaneMove, PaneOutcome, PressingLink, SearchQuery,
};

impl AppServices {
    /// Import the candidate the pane shows.
    pub async fn pane_start_import(&self, candidate_key: &str) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        import
            .run_pane_command(
                candidate_key,
                PaneCommand::Import,
                import.start_import(candidate_key),
            )
            .await
    }

    /// Take the two library artists the candidate's import found to be one as
    /// one, keeping `surviving_artist_id`.
    pub async fn pane_merge_artists(
        &self,
        candidate_key: &str,
        surviving_artist_id: &str,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        import
            .run_pane_command(
                candidate_key,
                PaneCommand::MergeArtists,
                import.merge_candidate_artist_identity_conflict(candidate_key, surviving_artist_id),
            )
            .await
    }

    /// Link the candidate to the release `link` names and read its draft from
    /// it. A catalog release that fails to load is told on that release's own
    /// row, so its failure comes back as the error.
    pub async fn pane_select_release(
        &self,
        candidate_key: String,
        link: PressingLink,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        import.clear_pane_failure(&candidate_key).await?;
        import
            .select_candidate_release(candidate_key.clone(), link)
            .await?;
        // A pick that landed leaves the draft it read to see.
        import
            .move_candidate_pane(&candidate_key, PaneMove::Picked)
            .await?;
        Ok(PaneOutcome::Done)
    }

    /// Read the candidate's draft from its files' own tags, a pane command
    /// whose failure the pane states. The release link stays as it is.
    pub async fn pane_read_file_tags(
        &self,
        candidate_key: String,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        let outcome = import
            .run_pane_command(
                &candidate_key,
                PaneCommand::ReadFileTags,
                import.select_candidate_file_tags(candidate_key.clone()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            import
                .move_candidate_pane(&candidate_key, PaneMove::Picked)
                .await?;
        }
        Ok(outcome)
    }

    /// Unlink the candidate from its release, leaving its draft as it is.
    pub async fn pane_unlink_release(
        &self,
        candidate_key: &str,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        import
            .run_pane_command(
                candidate_key,
                PaneCommand::Unlink,
                import.unlink_candidate_release(candidate_key.to_string()),
            )
            .await
    }

    /// Keep the candidate's own draft over what its lookup offered, and go
    /// back to the draft.
    pub async fn pane_keep_own_draft(
        &self,
        candidate_key: &str,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        let outcome = import
            .run_pane_command(
                candidate_key,
                PaneCommand::KeepOwnDraft,
                import.keep_candidate_draft(candidate_key.to_string()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            import
                .move_candidate_pane(candidate_key, PaneMove::Picked)
                .await?;
        }
        Ok(outcome)
    }

    /// Link the candidate to the album the pressings its lookup offers are
    /// of, its pressing unknown, taking what they agree on into its draft,
    /// and go back to the draft.
    pub async fn pane_link_shared_album(
        &self,
        candidate_key: &str,
    ) -> Result<PaneOutcome, ImportError> {
        let import = &self.inner.import;
        let outcome = import
            .run_pane_command(
                candidate_key,
                PaneCommand::LinkSharedAlbum,
                import.link_candidate_shared_album(candidate_key.to_string()),
            )
            .await?;
        if outcome == PaneOutcome::Done {
            import
                .move_candidate_pane(candidate_key, PaneMove::Picked)
                .await?;
        }
        Ok(outcome)
    }

    /// Make one change to what the candidate's identification asks about or
    /// counts, and run it again when what it looks up changed.
    pub async fn pane_edit_lookup_choices(
        &self,
        candidate_key: String,
        edit: LookupChoiceEdit,
    ) -> Result<PaneOutcome, ImportError> {
        let command = match &edit {
            LookupChoiceEdit::ToggleDiscId
            | LookupChoiceEdit::ToggleBarcode { .. }
            | LookupChoiceEdit::ToggleCatalog { .. } => PaneCommand::ChangeLookups,
            LookupChoiceEdit::SearchBy { .. } => PaneCommand::ChangeSearchWords,
            LookupChoiceEdit::ToggleDiscounted { .. } => PaneCommand::ChangeAgreements,
        };
        let key = candidate_key.clone();
        self.inner
            .import
            .run_pane_command(
                &key,
                command,
                self.edit_candidate_lookup_choices(candidate_key, edit),
            )
            .await
    }

    /// Show identification's results for the candidate: the stored verdict as
    /// it stood, picked row and all, when there is one, and a run started
    /// when there is none. A stored verdict is not asked again: the person
    /// looks at what it found, and picks from it.
    pub async fn pane_identify_automatically(
        &self,
        candidate_key: String,
    ) -> Result<(), ImportError> {
        if self.inner.import.open_automatic(&candidate_key).await? {
            self.rerun_identify(candidate_key);
        }
        Ok(())
    }

    /// Submit the candidate's typed search. What it turns up, failures
    /// included, lands on the candidate's runtime rather than the pane's
    /// banner, so this only clears the banner for it.
    pub async fn pane_start_candidate_search(
        &self,
        candidate_key: String,
        query: SearchQuery,
    ) -> Result<(), ImportError> {
        self.inner.import.clear_pane_failure(&candidate_key).await?;
        self.inner
            .import
            .start_candidate_search(candidate_key, query);
        Ok(())
    }

    /// Make one change to what a candidate's identification asks about, and
    /// run it again when what it looks up has changed: a run takes its choices
    /// at its start, so a person changing one is asking for a run that reads
    /// it. Any run already going for this candidate is superseded.
    ///
    /// Striking a number out of the candidate's text asks nothing of the
    /// providers — the answers in hand are the same answers, ranked by what
    /// the folder is now taken to state about them — so it starts no run, and
    /// the next read of the candidate ranks them afresh.
    async fn edit_candidate_lookup_choices(
        &self,
        candidate_key: String,
        edit: crate::import::LookupChoiceEdit,
    ) -> Result<(), crate::import::ImportError> {
        let change = self
            .inner
            .import
            .edit_candidate_lookup_choices(&candidate_key, edit)
            .await?;
        if change == crate::import::ChoiceChange::Lookups {
            self.inner.identification.rerun_identify(candidate_key);
        }
        Ok(())
    }
}
