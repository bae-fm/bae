//! What a candidate's identification asks about, written as the person
//! decides it.

use super::ImportServiceHandle;
use crate::import::{ChoiceChange, LookupChoices};

impl ImportServiceHandle {
    /// Record what this candidate's identification asks about — the whole
    /// value, never a flip of one part of it. The runs that follow read it;
    /// this write does not start one, and what comes back says whether one is
    /// owed: the lookups changing means the answers in hand were produced by
    /// a question nobody is asking any more.
    ///
    /// Under the commit lock, so a write cannot land between another's read of
    /// the candidate and its own write — which is also what makes the value it
    /// replaced the one the caller is told about.
    pub async fn set_candidate_lookup_choices(
        &self,
        candidate_key: &str,
        choices: LookupChoices,
    ) -> Result<ChoiceChange, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_lookup_choices_write(&candidate_key, choices)
                .await
        })
        .await
    }

    async fn set_candidate_lookup_choices_write(
        &self,
        candidate_key: &str,
        choices: LookupChoices,
    ) -> Result<ChoiceChange, crate::import::ImportError> {
        let _commit = self.folder_state_commit.lock().await;
        let projection = self
            .library_manager
            .load_import_candidate(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not a scanned folder candidate"),
            })?;
        let content_hash = projection.candidate.files().content_hash();
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
