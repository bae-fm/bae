//! Applying a metadata source to a candidate: a pick, a File Tags reading,
//! or a blank draft. Each replaces the draft, its provenance, its cover, and
//! the prepared answers as one unit, and leaves the file decisions alone.

use super::*;
use crate::import::preparation::CandidatePreparation;
use crate::import::MetadataAuthor;

impl Database {
    /// Replace the candidate's draft and its provenance as one transaction,
    /// carrying the stored rows' file decisions onto the new tracks. File
    /// decisions about the folder itself live in other tables and are
    /// deliberately untouched.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn replace_candidate_metadata(
        &self,
        content_hash: &str,
        folder_path: &str,
        draft: &crate::import::RawReleaseEdit,
        provenance: Option<&crate::import::MetadataProvenance>,
    ) -> Result<u64, DbError> {
        let prep = self
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                DbError::Message("metadata replacement has no candidate state row".into())
            })?;
        let mut draft = crate::import::pane::candidate_draft_from_edit(draft.clone()).draft;
        draft.tracks =
            crate::import::preserve_track_decisions(draft.tracks, &prep.metadata.draft.tracks);
        let metadata = crate::import::CandidateMetadataDraft {
            draft,
            source_discogs_artist_ids: Default::default(),
            provenance: provenance.cloned(),
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(prep, None, folder_path, metadata, None)
            .await
    }

    pub async fn replace_candidate_metadata_prepared(
        &self,
        watched_folder_path: &str,
        content_hash: &str,
        folder_path: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        metadata: &crate::import::CandidateMetadataDraft,
    ) -> Result<u64, DbError> {
        let prep = self
            .loaded_at(content_hash, expected_file_edit_revision, expected_revision)
            .await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: folder_path.to_string(),
        };
        self.apply_metadata(prep, Some(scanned), folder_path, metadata.clone(), None)
            .await
    }

    /// Store the exact File Tags reading and replace the candidate metadata it
    /// projects in one transaction. The scan stamp is checked inside that
    /// transaction, so no draft can be committed from facts about an older
    /// candidate shape.
    pub(crate) async fn replace_candidate_file_tags_metadata(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_metadata_revision: u64,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
        draft: &crate::import::CandidateDraft,
        cover: Option<&crate::import::CoverSelection>,
    ) -> Result<u64, DbError> {
        let prep = self
            .loaded_at(
                content_hash,
                expected_file_edit_revision,
                expected_metadata_revision,
            )
            .await?;
        let scanned = ScannedCandidateKey {
            watched_folder_path: watched_folder_path.to_string(),
            candidate_path: candidate_path.to_string(),
        };
        let metadata = crate::import::CandidateMetadataDraft {
            draft: draft.clone(),
            source_discogs_artist_ids: Default::default(),
            provenance: Some(crate::import::MetadataProvenance::FileTags),
            cover: cover.cloned(),
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.apply_metadata(
            prep,
            Some(scanned),
            candidate_path,
            metadata,
            Some(snapshot.clone()),
        )
        .await
    }

    /// The candidate at exactly the revisions a source projection was
    /// prepared against, or the refusal naming which one moved.
    async fn loaded_at(
        &self,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_metadata_revision: u64,
    ) -> Result<CandidatePreparation, DbError> {
        let prep = self
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| DbError::Message("candidate metadata row is missing".into()))?;
        if prep.file_edits.revision != expected_file_edit_revision {
            return Err(DbError::Message(format!(
                "candidate changed before its metadata was stored: its files moved past \
                 revision {expected_file_edit_revision}"
            )));
        }
        if prep.metadata_revision != expected_metadata_revision {
            return Err(DbError::Message(format!(
                "candidate metadata changed from revision {expected_metadata_revision}"
            )));
        }
        Ok(prep)
    }

    /// A source's projection becomes the candidate's metadata: the person
    /// applied it, so they are its author, and its answers are complete.
    async fn apply_metadata(
        &self,
        mut prep: CandidatePreparation,
        scanned: Option<ScannedCandidateKey>,
        folder_path: &str,
        metadata: crate::import::CandidateMetadataDraft,
        file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
    ) -> Result<u64, DbError> {
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned,
        };
        prep.folder_path = folder_path.to_string();
        prep.author = match metadata.provenance {
            Some(_) => MetadataAuthor::User,
            None => MetadataAuthor::Nobody,
        };
        prep.metadata = metadata;
        prep.assets_prepared = true;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        let extras = CandidateSaveExtras {
            file_tag_snapshot,
            reshaped_files: None,
        };
        match self
            .save_candidate_preparation(prep, expected, extras)
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(DbError::Message(
                "candidate changed before its metadata was stored".into(),
            )),
        }
    }
}
