use super::*;

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
        let state = self
            .load_import_candidate_state(content_hash)
            .await?
            .ok_or_else(|| {
                DbError::Message("metadata replacement has no candidate state row".into())
            })?;
        let expected_file_edit_revision = state.file_edits.revision;
        let expected_revision = state.metadata_revision;
        let current = self
            .load_import_candidate_pane_rows(content_hash)
            .await?
            .draft;
        let mut draft = crate::import::pane::candidate_draft_from_edit(draft.clone()).draft;
        draft.tracks = crate::import::preserve_track_decisions(draft.tracks, &current.tracks);
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let metadata = crate::import::CandidateMetadataDraft {
            draft,
            source_discogs_artist_ids: Default::default(),
            provenance: provenance.cloned(),
            cover: None,
            assets: crate::import::CandidatePreparedAssets::default(),
        };
        self.call(move |sql| {
            pane_rows::require_file_edit_revision(sql, &content_hash, expected_file_edit_revision)?;
            replace_candidate_metadata_on(
                sql,
                &content_hash,
                &folder_path,
                expected_revision,
                &metadata,
            )
        })
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
        let watched_folder_path = watched_folder_path.to_string();
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let metadata = metadata.clone();
        self.call(move |sql| {
            require_current_candidate(
                sql,
                &watched_folder_path,
                &folder_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            pane_rows::require_file_edit_revision(sql, &content_hash, expected_file_edit_revision)?;
            replace_candidate_metadata_on(
                sql,
                &content_hash,
                &folder_path,
                expected_revision,
                &metadata,
            )
        })
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
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let snapshot = snapshot.clone();
        let draft = draft.clone();
        let cover = cover.cloned();
        self.call(move |sql| {
            pane_rows::require_metadata_revision(sql, &content_hash, expected_metadata_revision)?;
            let current_generation = require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            if snapshot.file_edit_revision != expected_file_edit_revision
                || snapshot.scan_generation != current_generation
            {
                return Err(DbError::Message(format!(
                    "candidate {candidate_path} changed before its file tags were stored"
                )));
            }
            super::folder_scans::write::replace_candidate_file_tag_snapshot(
                sql,
                &watched_folder_path,
                &candidate_path,
                &snapshot,
            )?;
            let revision = sql.query_row(
                "UPDATE import_candidate_state SET folder_path = ?, \
                     provenance_kind = 'file_tags', provenance_source = NULL, \
                     provenance_release_id = NULL, provenance_author = 'user', \
                     metadata_revision = metadata_revision + 1 \
                 WHERE content_hash = ? RETURNING metadata_revision",
                params![candidate_path, content_hash],
                |row| row.get::<_, i64>(0),
            )?;
            replace_provenance_partners(
                sql,
                &content_hash,
                Some(&crate::import::MetadataProvenance::FileTags),
            )?;
            pane_rows::replace_draft(sql, &content_hash, &draft)?;
            pane_rows::delete_cover(sql, &content_hash)?;
            if let Some(cover) = &cover {
                super::candidate_state_rows::save_cover(sql, &content_hash, cover)?;
            }
            replace_prepared_assets(
                sql,
                &content_hash,
                cover.as_ref(),
                &Default::default(),
                &crate::import::CandidatePreparedAssets::default(),
            )?;
            u64::try_from(revision)
                .map_err(|_| DbError::Message("candidate metadata revision is negative".into()))
        })
        .await
    }
}
