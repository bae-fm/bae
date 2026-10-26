//! The pane's own writes: the cover, the album fields, and the track rows.
//! Each loads the candidate, changes the one thing the control changed, and
//! saves it whole under the next metadata revision.

use super::CandidatePreparations;
use crate::db::{
    CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved, ScannedCandidateKey,
};
use crate::import::preparation::{CandidateAsRead, CandidatePreparation, CandidateWrite};
use crate::import::{CandidateDraft, TrackArtistAssignments};
use crate::library::LibraryError;

impl CandidatePreparations {
    /// Record the cover the user chose for this candidate, with the bytes
    /// fetched for a remote one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_cover(
        &self,
        content_hash: &str,
        cover: &crate::import::CoverSelection,
        remote_image: Option<&bae_catalogs::remote_image::RemoteImage>,
    ) -> Result<u64, LibraryError> {
        let cover = cover.clone();
        let remote_image = remote_image.cloned();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            prep.metadata.cover = Some(cover);
            prep.metadata.assets.remote_cover = remote_image;
            Ok(())
        })
        .await
    }

    pub async fn set_prepared_cover(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        cover: &crate::import::CoverSelection,
        remote_image: Option<&bae_catalogs::remote_image::RemoteImage>,
    ) -> Result<u64, LibraryError> {
        let cover = cover.clone();
        let remote_image = remote_image.cloned();
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            &read.content_hash,
            Some(read.file_edit_revision),
            Some(read.metadata_revision),
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
    pub async fn set_field(
        &self,
        content_hash: &str,
        field: crate::import::CandidateEditField,
        value: &str,
    ) -> Result<u64, LibraryError> {
        self.set_field_edit(
            content_hash,
            crate::import::DraftFieldEdit::Text {
                field,
                value: value.to_string(),
            },
        )
        .await
    }

    /// Record one album-level field the user typed or chose.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_field_edit(
        &self,
        content_hash: &str,
        edit: crate::import::DraftFieldEdit,
    ) -> Result<u64, LibraryError> {
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            edit.apply(&mut prep.metadata.draft);
            Ok(())
        })
        .await
    }

    /// An album field, typed or chosen, which stands on its own: it carries no prepared
    /// answers and does not read the rest of the draft, so it pins the file
    /// shape it was typed over and nothing else. That is why it takes the
    /// hash and the file revision rather than a whole
    /// [`CandidateAsRead`] — its caller never reads a metadata revision.
    pub async fn set_field_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        expected_file_edit_revision: u64,
        edit: crate::import::DraftFieldEdit,
    ) -> Result<u64, LibraryError> {
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            content_hash,
            Some(expected_file_edit_revision),
            None,
            move |prep| {
                edit.apply(&mut prep.metadata.draft);
                Ok(())
            },
        )
        .await
    }

    /// Replace the candidate's ordered album-artist override. An empty list is
    /// rejected because every savable album has an artist; deleting the whole
    /// candidate edit resets the override to its metadata draft.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_album_artists(
        &self,
        content_hash: &str,
        assignments: &[crate::import::Credited],
    ) -> Result<u64, LibraryError> {
        if assignments.is_empty() {
            return Err(LibraryError::Refused(
                "a candidate album artist override cannot be empty".into(),
            ));
        }
        let assignments = assignments.to_vec();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            prep.metadata.draft.album_artist_assignments = assignments;
            Ok(())
        })
        .await
    }

    pub async fn set_album_artists_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        assignments: &[crate::import::Credited],
    ) -> Result<u64, LibraryError> {
        if assignments.is_empty() {
            return Err(LibraryError::Refused(
                "a candidate album artist override cannot be empty".into(),
            ));
        }
        let assignments = assignments.to_vec();
        self.edit_read_candidate(watched_folder_path, candidate_path, read, move |prep| {
            prep.metadata.draft.album_artist_assignments = assignments;
            Ok(())
        })
        .await
    }

    /// Record one mapping-table row the user changed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_track_edit(
        &self,
        content_hash: &str,
        row: &crate::import::RawTrackEdit,
    ) -> Result<u64, LibraryError> {
        let row = row.clone();
        self.edit_candidate(None, content_hash, None, None, move |prep| {
            apply_track_edit(&mut prep.metadata.draft, &row)
        })
        .await
    }

    /// Record one mapping-table row the user changed.
    pub async fn set_track_edit_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        row: &crate::import::RawTrackEdit,
    ) -> Result<u64, LibraryError> {
        self.edit_read_candidate(watched_folder_path, candidate_path, read, |prep| {
            apply_track_edit(&mut prep.metadata.draft, row)
        })
        .await
    }

    pub async fn set_track_artists_prepared(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        track_ids: &[String],
        assignments: &TrackArtistAssignments,
    ) -> Result<u64, LibraryError> {
        if track_ids.is_empty() {
            return Err(LibraryError::Refused(
                "a track artist fill must name at least one track".into(),
            ));
        }
        self.edit_read_candidate(watched_folder_path, candidate_path, read, |prep| {
            fill_track_artists(&mut prep.metadata.draft, track_ids, assignments)
        })
        .await
    }

    /// A pane edit composed against the candidate as `read`: refused when
    /// the scan no longer lists it at that file revision, or either revision
    /// moved.
    async fn edit_read_candidate(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        read: &CandidateAsRead,
        change: impl FnOnce(&mut CandidatePreparation) -> Result<(), LibraryError>,
    ) -> Result<u64, LibraryError> {
        self.edit_candidate(
            Some(scanned_key(watched_folder_path, candidate_path)),
            &read.content_hash,
            Some(read.file_edit_revision),
            Some(read.metadata_revision),
            change,
        )
        .await
    }

    /// Load one candidate, change its metadata, and save it whole under the
    /// next metadata revision, as the person's draft.
    ///
    /// `scanned` names where the scan must still list the candidate at the
    /// expected file revision; the revision expectations are checked against
    /// the loaded value, and the save re-checks them in its transaction. An
    /// expectation left `None` is the loaded value itself: a typed field is
    /// independent of the draft it is typed over, so it pins the file shape
    /// alone, and the test-only writers pin neither.
    async fn edit_candidate(
        &self,
        scanned: Option<ScannedCandidateKey>,
        content_hash: &str,
        expected_file_edit_revision: Option<u64>,
        expected_metadata_revision: Option<u64>,
        change: impl FnOnce(&mut CandidatePreparation) -> Result<(), LibraryError>,
    ) -> Result<u64, LibraryError> {
        let mut prep = self
            .database
            .load_candidate_preparation(content_hash)
            .await?
            .ok_or_else(|| {
                LibraryError::Refused(format!(
                    "the metadata edit for {content_hash} has no candidate state row"
                ))
            })?;
        if let Some(expected) = expected_file_edit_revision {
            if prep.file_edits.revision != expected {
                return Err(CandidateAsRead::files_moved(
                    expected,
                    CandidateWrite::PaneEdit,
                ));
            }
        }
        if let Some(expected) = expected_metadata_revision {
            if prep.metadata_revision != expected {
                return Err(CandidateAsRead::metadata_moved(expected));
            }
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned,
        };
        change(&mut prep)?;
        // An edit is the person working on the draft, whoever wrote it before:
        // after it, the draft is their answer rather than what identification
        // or the folder's tags proposed.
        prep.author = crate::import::MetadataAuthor::Person;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        match self
            .database
            .save_candidate_preparation(prep, expected, CandidateSaveExtras::default())
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => Err(LibraryError::Refused(format!(
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

/// Replace one row's title and artists. Its audio is the folder's, so a row
/// edit that changes it is refused; its place is not in the edit at all.
fn apply_track_edit(
    draft: &mut CandidateDraft,
    row: &crate::import::RawTrackEdit,
) -> Result<(), LibraryError> {
    let track = draft
        .tracks
        .iter_mut()
        .find(|track| track.edit.id == row.id)
        .ok_or_else(|| LibraryError::Refused(format!("{} is not a row of this draft", row.id)))?;
    if row.file.as_ref() != Some(&track.edit.file) {
        return Err(LibraryError::Refused(
            "track metadata edits cannot change a track's audio".into(),
        ));
    }
    track.edit.title.clone_from(&row.title);
    track
        .edit
        .artist_assignments
        .clone_from(&row.artist_assignments);
    Ok(())
}

fn fill_track_artists(
    draft: &mut CandidateDraft,
    track_ids: &[String],
    assignments: &TrackArtistAssignments,
) -> Result<(), LibraryError> {
    for track_id in track_ids {
        let track = draft
            .tracks
            .iter_mut()
            .find(|track| &track.edit.id == track_id)
            .ok_or_else(|| {
                LibraryError::Refused(format!(
                    "track artist fill names {track_id}, which is not a row of this draft"
                ))
            })?;
        track.edit.artist_assignments = assignments.clone();
    }
    Ok(())
}
