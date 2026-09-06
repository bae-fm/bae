use super::*;

impl Database {
    pub async fn load_import_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbCandidateImportPreparation>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            let Some(state) =
                super::rows::load_states_on(&sql, Some(&content_hash))?.remove(&content_hash)
            else {
                return Ok(None);
            };
            let rows = load_pane_rows_on(&sql, &content_hash)?;
            let source_discogs_artist_ids =
                super::prepared_asset_rows::load_source_artist_ids_on(&sql, &content_hash)?;
            let assets = super::prepared_asset_rows::load_prepared_assets_on(
                &sql,
                &content_hash,
                rows.cover.as_ref(),
            )?;
            Ok(Some(crate::db::DbCandidateImportPreparation {
                file_edit_revision: state.file_edits.revision,
                metadata_revision: state.metadata_revision,
                metadata_provenance: state.metadata_provenance,
                cover: rows.cover,
                draft: rows.draft,
                source_discogs_artist_ids,
                assets,
            }))
        })
        .await
    }

    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_pane_rows_on(&sql, &content_hash))
            .await
    }

    pub async fn load_import_candidate_prepared_assets(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::CandidatePreparedAssets, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            let rows = load_pane_rows_on(&sql, &content_hash)?;
            super::prepared_asset_rows::load_prepared_assets_on(
                &sql,
                &content_hash,
                rows.cover.as_ref(),
            )
        })
        .await
    }

    /// Forget the last failure — what queueing an import of this candidate
    /// does before the worker takes it.
    pub async fn clear_import_candidate_failure(&self, content_hash: &str) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.call(move |sql| {
            sql.execute(
                "DELETE FROM import_candidate_failure WHERE content_hash = ?",
                [&content_hash],
            )?;
            Ok(())
        })
        .await
    }

    /// Record the cover the user chose for this candidate.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_import_candidate_cover(
        &self,
        content_hash: &str,
        cover: &crate::import::CoverSelection,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let cover = cover.clone();
        self.call(move |sql| {
            save_cover(sql, &content_hash, &cover)?;
            sql.execute(
                "DELETE FROM import_candidate_remote_cover_asset WHERE content_hash = ?",
                [&content_hash],
            )?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    pub async fn save_import_candidate_prepared_cover(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        cover: &crate::import::CoverSelection,
        remote_image: Option<&crate::import::cover_art::RemoteImage>,
    ) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let cover = cover.clone();
        let remote_image = remote_image.cloned();
        self.call(move |sql| {
            super::require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            require_metadata_revision(sql, &content_hash, expected_revision)?;
            save_cover(sql, &content_hash, &cover)?;
            super::prepared_asset_rows::replace_remote_cover_asset(
                sql,
                &content_hash,
                Some(&cover),
                remote_image.as_ref(),
            )?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Record one album-level field the user typed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_import_candidate_edit_field(
        &self,
        content_hash: &str,
        field: crate::import::CandidateEditField,
        value: &str,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let value = value.to_string();
        self.call(move |sql| save_edit_field_and_advance(sql, &content_hash, field, &value))
            .await
    }

    pub async fn save_import_candidate_edit_field_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        field: crate::import::CandidateEditField,
        value: &str,
    ) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let value = value.to_string();
        self.call(move |sql| {
            super::require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            save_edit_field_and_advance(sql, &content_hash, field, &value)
        })
        .await
    }

    /// Replace the candidate's ordered album-artist override. An empty list is
    /// rejected because every savable album has an artist; deleting the whole
    /// candidate edit resets the override to its metadata draft.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn replace_import_candidate_album_artists(
        &self,
        content_hash: &str,
        assignments: &[crate::import::ArtistAssignment],
    ) -> Result<u64, DbError> {
        if assignments.is_empty() {
            return Err(DbError::Message(
                "a candidate album artist override cannot be empty".into(),
            ));
        }
        let content_hash = content_hash.to_string();
        let assignments = assignments.to_vec();
        self.call(move |sql| {
            require_state_row(sql, &content_hash, "album artist edit")?;
            sql.execute(
                "DELETE FROM import_candidate_album_artist_assignment WHERE content_hash = ?",
                [&content_hash],
            )?;
            insert_album_artist_assignments(sql, &content_hash, &assignments)?;
            invalidate_prepared_assets(sql, &content_hash)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    pub async fn replace_import_candidate_album_artists_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        assignments: &[crate::import::ArtistAssignment],
        source_discogs_artist_ids: &std::collections::BTreeSet<String>,
        assets: &[crate::import::PreparedArtistImage],
    ) -> Result<u64, DbError> {
        if assignments.is_empty() {
            return Err(DbError::Message(
                "a candidate album artist override cannot be empty".into(),
            ));
        }
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let assignments = assignments.to_vec();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.call(move |sql| {
            super::require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            require_file_edit_revision(sql, &content_hash, expected_file_edit_revision)?;
            require_metadata_revision(sql, &content_hash, expected_revision)?;
            sql.execute(
                "DELETE FROM import_candidate_album_artist_assignment WHERE content_hash = ?",
                [&content_hash],
            )?;
            insert_album_artist_assignments(sql, &content_hash, &assignments)?;
            super::prepared_asset_rows::replace_artist_assets_for_stored_draft(
                sql,
                &content_hash,
                &source_discogs_artist_ids,
                &assets,
            )?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Record one mapping-table row the user changed, or dropped.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_import_candidate_track_edit(
        &self,
        content_hash: &str,
        edit: &crate::import::CandidateTrackEdit,
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let edit = edit.clone();
        self.call(move |sql| {
            save_track_edit(sql, &content_hash, &edit)?;
            invalidate_prepared_assets(sql, &content_hash)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Record the mapping-table rows one gesture changed, as one write: a
    /// plain row edit is one entry, and a file choice that displaces another
    /// row's audio is two. The rows land together or not at all, so the table
    /// can never be read with only half a swap applied.
    pub async fn save_import_candidate_track_edits_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        edits: &[crate::import::CandidateTrackEdit],
        source_discogs_artist_ids: &std::collections::BTreeSet<String>,
        assets: &[crate::import::PreparedArtistImage],
    ) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let edits = edits.to_vec();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.call(move |sql| {
            super::require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            require_file_edit_revision(sql, &content_hash, expected_file_edit_revision)?;
            require_metadata_revision(sql, &content_hash, expected_revision)?;
            for edit in &edits {
                save_track_edit(sql, &content_hash, edit)?;
            }
            super::prepared_asset_rows::replace_artist_assets_for_stored_draft(
                sql,
                &content_hash,
                &source_discogs_artist_ids,
                &assets,
            )?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    /// Replace the artist assignments of every named track in one transaction.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn replace_import_candidate_track_artists(
        &self,
        content_hash: &str,
        track_ids: &[String],
        assignments: &TrackArtistAssignments,
    ) -> Result<u64, DbError> {
        if track_ids.is_empty() {
            return Err(DbError::Message(
                "a track artist fill must name at least one track".into(),
            ));
        }
        let content_hash = content_hash.to_string();
        let track_ids = track_ids.to_vec();
        let assignments = assignments.clone();
        self.call(move |sql| {
            replace_track_artist_assignments(sql, &content_hash, &track_ids, &assignments)?;
            invalidate_prepared_assets(sql, &content_hash)?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }

    pub async fn replace_import_candidate_track_artists_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_revision: u64,
        track_ids: &[String],
        assignments: &TrackArtistAssignments,
        source_discogs_artist_ids: &std::collections::BTreeSet<String>,
        assets: &[crate::import::PreparedArtistImage],
    ) -> Result<u64, DbError> {
        if track_ids.is_empty() {
            return Err(DbError::Message(
                "a track artist fill must name at least one track".into(),
            ));
        }
        let watched_folder_path = watched_folder_path.to_string();
        let candidate_path = candidate_path.to_string();
        let content_hash = content_hash.to_string();
        let track_ids = track_ids.to_vec();
        let assignments = assignments.clone();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.call(move |sql| {
            super::require_current_candidate(
                sql,
                &watched_folder_path,
                &candidate_path,
                &content_hash,
                expected_file_edit_revision,
            )?;
            require_file_edit_revision(sql, &content_hash, expected_file_edit_revision)?;
            require_metadata_revision(sql, &content_hash, expected_revision)?;
            replace_track_artist_assignments(sql, &content_hash, &track_ids, &assignments)?;
            super::prepared_asset_rows::replace_artist_assets_for_stored_draft(
                sql,
                &content_hash,
                &source_discogs_artist_ids,
                &assets,
            )?;
            advance_metadata_revision(sql, &content_hash)
        })
        .await
    }
}
