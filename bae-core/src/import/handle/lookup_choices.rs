//! What a candidate's identification asks about, changed as the person
//! decides it.

use super::ImportServiceHandle;
use crate::import::{ChoiceChange, LookupChoiceEdit};

impl ImportServiceHandle {
    /// Make one change to what this candidate's identification asks about,
    /// to the choices stored for it. The runs that follow read them; this
    /// write does not start one, and what comes back says whether one is
    /// owed: the lookups changing means the answers in hand were produced by
    /// a question nobody is asking any more.
    ///
    /// Under the commit lock, so a change cannot land between another's read
    /// of the stored choices and its write: each change is made to the value
    /// the one before it left.
    pub async fn edit_candidate_lookup_choices(
        &self,
        candidate_key: &str,
        edit: LookupChoiceEdit,
    ) -> Result<ChoiceChange, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.edit_candidate_lookup_choices_write(&candidate_key, edit)
                .await
        })
        .await
    }

    async fn edit_candidate_lookup_choices_write(
        &self,
        candidate_key: &str,
        edit: LookupChoiceEdit,
    ) -> Result<ChoiceChange, crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock("store lookup choices").await;
        let projection = self
            .library_manager
            .load_import_candidate(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            })?;
        let content_hash = projection.candidate.files.content_hash();
        let choices = projection.lookup_choices.clone().edited(edit);
        let change = match choices.asks_the_same_as(&projection.lookup_choices) {
            true => ChoiceChange::Ranking,
            false => ChoiceChange::Lookups,
        };
        self.library_manager
            .save_import_candidate_lookup_choices(&content_hash, &choices)
            .await?;
        Ok(change)
    }
}
