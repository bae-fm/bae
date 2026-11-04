//! The person's answer when an import meets an artist or label the library
//! already holds under another identity: which one the candidate's import
//! links to, stored while the candidate is editable.

use super::*;

impl ImportServiceHandle {
    /// Store the person's artist answer while this candidate is editable.
    pub async fn choose_candidate_artist_identity(
        &self,
        candidate_key: &crate::CandidateKey,
        metadata_revision: u64,
        choice: crate::ArtistIdentityConflictChoice,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let key = candidate_key.clone();
        self.committed(async move {
            let _commit = this.folder_state_commit.lock("choose an import artist").await;
            let candidate = this.editable_candidate_for_commit(&key).await?;
            this.tables
                .choose_import_artist_identity(
                    &candidate.files.content_hash(),
                    metadata_revision,
                    choice,
                    this.ids.as_ref(),
                )
                .await?;
            Ok(())
        })
        .await
    }

    /// Store the person's label answer while this candidate is editable.
    pub async fn choose_candidate_label_identity(
        &self,
        candidate_key: &crate::CandidateKey,
        metadata_revision: u64,
        choice: crate::LabelIdentityConflictChoice,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let key = candidate_key.clone();
        self.committed(async move {
            let _commit = this.folder_state_commit.lock("choose an import label").await;
            let candidate = this.editable_candidate_for_commit(&key).await?;
            this.tables
                .choose_import_label_identity(
                    &candidate.files.content_hash(),
                    metadata_revision,
                    choice,
                    this.ids.as_ref(),
                )
                .await?;
            Ok(())
        })
        .await
    }
}
