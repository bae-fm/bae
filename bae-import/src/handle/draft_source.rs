//! The draft's source, short of a pick: clearing the draft, keeping it over
//! what a lookup offered, and unlinking its release; the stored preparation
//! each draft write starts from; and the announcement each one makes.

use super::*;

impl ImportServiceHandle {
    /// The candidate's stored preparation, for a write about to replace its
    /// draft.
    pub(super) async fn metadata_write_base(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<
        (crate::folder_scanner::FolderCandidate, crate::preparations::CandidatePreparation),
        crate::ImportError,
    > {
        let candidate = self.require_release_candidate(candidate_key).await?;
        let current =
            crate::preparations::stored_preparation(&self.tables, &candidate.files.content_hash())
                .await?
                .ok_or_else(|| crate::ImportError::Internal {
                    detail: format!("{candidate_key} has no stored import preparation"),
                })?;
        Ok((candidate, current))
    }

    /// Clear the candidate's draft, keeping its file decisions and its release
    /// link. Like a pick, it ends the candidate's identification and announces
    /// the change.
    pub async fn clear_candidate_metadata(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.clear_candidate_metadata_write(candidate_key.clone()).await?;
            this.identification_ended(&candidate_key).await;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn clear_candidate_metadata_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let mut draft = candidate.blank_draft();
        crate::pane::keep_row_identities(&mut draft.tracks, &current.metadata.draft.tracks)?;
        let _commit = self
            .commit_lock_for_revision(
                "clear metadata",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .apply_source(
                &candidate.watched_folder_path,
                &crate::CandidateAsRead {
                    content_hash: content_hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: current.metadata_revision,
                },
                &candidate_key,
                &crate::CandidateMetadataDraft { draft, provenance: None, cover: None },
            )
            .await
    }

    /// Keep the candidate's own draft over what its stored lookup offered:
    /// none of the releases it found is the folder's, or it found none. The
    /// candidate is linked to no release and the draft stays as it is; the
    /// decision goes with the verdict, so identifying the candidate again
    /// starts without it.
    pub async fn keep_candidate_draft(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move { this.keep_candidate_draft_write(candidate_key).await }).await
    }

    async fn keep_candidate_draft_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
                "keep the folder's own draft",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .keep_own_draft(&crate::CandidateAsRead {
                content_hash: content_hash.clone(),
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await
    }

    /// Unlink the candidate from its release. The draft stays as it is.
    pub async fn unlink_candidate_release(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let this = self.clone();
        self.committed(async move {
            let revision = this.unlink_candidate_release_write(candidate_key.clone()).await?;
            this.announce_metadata_changed(candidate_key);
            Ok(revision)
        })
        .await
    }

    async fn unlink_candidate_release_write(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<u64, crate::ImportError> {
        let (candidate, current) = self.metadata_write_base(&candidate_key).await?;
        let content_hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
                "unlink the release",
                &candidate_key,
                &content_hash,
                current.file_edits.revision,
            )
            .await?;
        self.preparations
            .unlink(&crate::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision,
                metadata_revision: current.metadata_revision,
            })
            .await
    }

    pub(crate) fn announce_metadata_changed(&self, candidate_key: crate::CandidateKey) {
        self.event_tx
            .send(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key }));
    }
}

#[cfg(test)]
#[path = "draft_source_tests.rs"]
mod tests;
