//! The pane's own writes: the cover, the album fields, and the track rows.
//! Each loads the candidate, changes the one thing the control changed, and
//! saves it whole under the next metadata revision.

use super::*;
use crate::import::preparation::CandidatePreparation;
use crate::import::CandidateDraft;

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
        let cover = cover.clone();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            // Chosen without its bytes, a remote cover leaves the candidate
            // unprepared until a source is applied again.
            if matches!(cover, crate::import::CoverSelection::Remote(_, _)) {
                prep.assets_prepared = false;
            }
            prep.metadata.cover = Some(cover);
            prep.metadata.assets.remote_cover = None;
            Ok(())
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
        let cover = cover.clone();
        let remote_image = remote_image.cloned();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            Some(expected_revision),
            move |prep| {
                prep.metadata.cover = Some(cover);
                prep.metadata.assets.remote_cover = remote_image;
                Ok(())
            },
        )
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
        let value = value.to_string();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            field.set(&mut prep.metadata.draft, &value);
            Ok(())
        })
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
        let value = value.to_string();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            None,
            move |prep| {
                field.set(&mut prep.metadata.draft, &value);
                Ok(())
            },
        )
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
        let assignments = assignments.to_vec();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            prep.metadata.draft.album_artist_assignments = assignments;
            prep.assets_prepared = false;
            Ok(())
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
        let assignments = assignments.to_vec();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            Some(expected_revision),
            move |prep| {
                require_prepared(prep)?;
                prep.metadata.draft.album_artist_assignments = assignments;
                prep.metadata.source_discogs_artist_ids = source_discogs_artist_ids;
                prep.metadata.assets.artist_images = assets;
                Ok(())
            },
        )
        .await
    }

    /// Record one mapping-table row the user changed, or dropped.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_import_candidate_track_edit(
        &self,
        content_hash: &str,
        edit: &crate::import::CandidateTrackEdit,
    ) -> Result<u64, DbError> {
        let edit = edit.clone();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            apply_track_edit(&mut prep.metadata.draft, &edit)?;
            prep.assets_prepared = false;
            Ok(())
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
        let edits = edits.to_vec();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            Some(expected_revision),
            move |prep| {
                require_prepared(prep)?;
                for edit in &edits {
                    apply_track_edit(&mut prep.metadata.draft, edit)?;
                }
                prep.metadata.source_discogs_artist_ids = source_discogs_artist_ids;
                prep.metadata.assets.artist_images = assets;
                Ok(())
            },
        )
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
        let track_ids = track_ids.to_vec();
        let assignments = assignments.clone();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            fill_track_artists(&mut prep.metadata.draft, &track_ids, &assignments)?;
            prep.assets_prepared = false;
            Ok(())
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
        let track_ids = track_ids.to_vec();
        let assignments = assignments.clone();
        let source_discogs_artist_ids = source_discogs_artist_ids.clone();
        let assets = assets.to_vec();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            Some(expected_revision),
            move |prep| {
                require_prepared(prep)?;
                fill_track_artists(&mut prep.metadata.draft, &track_ids, &assignments)?;
                prep.metadata.source_discogs_artist_ids = source_discogs_artist_ids;
                prep.metadata.assets.artist_images = assets;
                Ok(())
            },
        )
        .await
    }

    /// Load one candidate, change its metadata, and save it whole under the
    /// next metadata revision.
    ///
    /// `scanned` names where the scan must still list the candidate at the
    /// expected file revision; the revision expectations are checked against
    /// the loaded value, and the save re-checks them in its transaction. An
    /// expectation left `None` is the loaded value itself.
    async fn edit_candidate(
        &self,
        scanned: Option<ScannedCandidateKey>,
        content_hash: &str,
        expected_file_edit_revision: Option<u64>,
        expected_metadata_revision: Option<u64>,
        change: impl FnOnce(&mut CandidatePreparation) -> Result<(), DbError>,
    ) -> Result<u64, DbError> {
        let mut prep = self
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                DbError::Message(format!(
                    "the metadata edit for {content_hash} has no candidate state row"
                ))
            })?;
        if let Some(expected) = expected_file_edit_revision {
            if prep.file_edits.revision != expected {
                return Err(DbError::Message(format!(
                    "candidate changed before its edit was stored: its files moved past \
                     revision {expected}"
                )));
            }
        }
        if let Some(expected) = expected_metadata_revision {
            if prep.metadata_revision != expected {
                return Err(DbError::Message(format!(
                    "candidate metadata changed from revision {expected}"
                )));
            }
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned,
        };
        change(&mut prep)?;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        match self
            .save_candidate_preparation(prep, expected, CandidateSaveExtras::default())
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(DbError::Message(format!(
                "candidate {content_hash} changed while its edit was being stored"
            ))),
        }
    }
}

fn scanned_key(watched_folder_path: &str, candidate_path: &str) -> ScannedCandidateKey {
    ScannedCandidateKey {
        watched_folder_path: watched_folder_path.to_string(),
        candidate_path: candidate_path.to_string(),
    }
}

/// A pane edit on a candidate whose answers were never prepared has nothing
/// to keep in step with; the source has to be applied again first.
fn require_prepared(prep: &CandidatePreparation) -> Result<(), DbError> {
    if prep.assets_prepared {
        Ok(())
    } else {
        Err(DbError::Message(format!(
            "candidate {} has no complete prepared asset set",
            prep.content_hash
        )))
    }
}

/// One row as the person left it, or dropped. A row edited back into the
/// import is undropped by the edit. A file that changed hands is the
/// person's choice from here on; one left alone keeps whoever chose it.
fn apply_track_edit(
    draft: &mut CandidateDraft,
    edit: &crate::import::CandidateTrackEdit,
) -> Result<(), DbError> {
    let track = draft
        .tracks
        .iter_mut()
        .find(|track| track.edit.id == edit.track_id)
        .ok_or_else(|| {
            DbError::Message(format!(
                "track decision edit names {}, which is not a row of this draft",
                edit.track_id
            ))
        })?;
    let previous_file = track.edit.file.clone();
    match &edit.state {
        crate::import::TrackEditState::Dropped => {
            track.dropped = true;
            track.edit.file = None;
        }
        crate::import::TrackEditState::Edited(row) => {
            track.dropped = false;
            track.edit = row.clone();
        }
    }
    if track.edit.file != previous_file {
        track.file_author = crate::import::TrackFileAuthor::User;
    }
    Ok(())
}

fn fill_track_artists(
    draft: &mut CandidateDraft,
    track_ids: &[String],
    assignments: &TrackArtistAssignments,
) -> Result<(), DbError> {
    for track_id in track_ids {
        let track = draft
            .tracks
            .iter_mut()
            .find(|track| &track.edit.id == track_id)
            .ok_or_else(|| {
                DbError::Message(format!(
                    "track artist fill names {track_id}, which is not a row of this draft"
                ))
            })?;
        track.edit.artist_assignments = assignments.clone();
    }
    Ok(())
}
