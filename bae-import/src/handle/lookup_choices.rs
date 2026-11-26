//! What a candidate's identification asks about, changed as the person
//! decides it.

use super::ImportServiceHandle;
use bae_identify::{ChoiceChange, LookupChoiceEdit};

impl ImportServiceHandle {
    /// Make one change to what this candidate's identification asks about,
    /// to the choices stored for it. The runs that follow read them; this
    /// write does not start one, and what comes back says whether one is
    /// owed: the lookups changing means the answers in hand were produced by
    /// a question nobody is asking any more.
    pub async fn edit_candidate_lookup_choices(
        &self,
        candidate_key: &crate::CandidateKey,
        edit: LookupChoiceEdit,
    ) -> Result<ChoiceChange, crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.services.tables.edit_pane_lookup_choices(&candidate_key, edit).await
        })
        .await
    }
}

#[cfg(test)]
#[path = "lookup_choices_tests.rs"]
mod tests;
