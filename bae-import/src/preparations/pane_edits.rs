//! The pane's own writes: the cover, the album fields, and the track rows.
//! Each loads the candidate, changes the one thing the control changed, and
//! saves it whole under the next metadata revision.

use crate::preparations::{CandidateAsRead, CandidatePreparation};
use crate::tables::CandidateStateRefusal;
use crate::tables::{
    CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved, ImportTables,
    ScannedCandidateKey,
};
use crate::{CandidateDraft, TrackArtistAssignments};

impl ImportTables {
    /// Record the cover the person chose for this candidate, with the bytes
    /// fetched for a remote one.
    pub async fn set_prepared_cover(
        &self,
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        cover: &crate::cover_art::PreparedCover,
    ) -> Result<u64, crate::ImportError> {
        let cover = cover.clone();
        self.edit_read_candidate(watched_folder_path, candidate_path, read, move |prep| {
            prep.metadata.cover = Some(cover);
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
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        content_hash: &str,
        expected_file_edit_revision: u64,
        edit: crate::DraftFieldEdit,
    ) -> Result<u64, crate::ImportError> {
        self.edit_candidate(
            scanned_key(watched_folder_path, candidate_path),
            content_hash,
            expected_file_edit_revision,
            None,
            move |prep| {
                edit.apply(&mut prep.metadata.draft);
                Ok(())
            },
        )
        .await
    }

    /// Replace the candidate's ordered album-artist override. An empty list is
    /// refused because every savable album has an artist; deleting the whole
    /// candidate edit resets the override to its metadata draft.
    pub async fn set_album_artists_prepared(
        &self,
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        assignments: &[crate::Credited],
    ) -> Result<u64, crate::ImportError> {
        if assignments.is_empty() {
            return Err(crate::EditValidationError::NoAlbumArtist.into());
        }
        let assignments = assignments.to_vec();
        self.edit_read_candidate(watched_folder_path, candidate_path, read, move |prep| {
            prep.metadata.draft.album_artist_assignments = assignments;
            Ok(())
        })
        .await
    }

    /// Record one mapping-table row the user changed.
    pub async fn set_track_edit_prepared(
        &self,
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        row: &crate::RawTrackEdit,
    ) -> Result<u64, crate::ImportError> {
        self.edit_read_candidate(watched_folder_path, candidate_path, read, |prep| {
            apply_track_edit(&mut prep.metadata.draft, row)
        })
        .await
    }

    pub async fn set_track_artists_prepared(
        &self,
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        track_ids: &[String],
        assignments: &TrackArtistAssignments,
    ) -> Result<u64, crate::ImportError> {
        if track_ids.is_empty() {
            return Err(TrackEditRefusal::NoTrackNamed.into());
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
        watched_folder_path: &std::path::Path,
        candidate_path: &crate::CandidateKey,
        read: &CandidateAsRead,
        change: impl FnOnce(&mut CandidatePreparation) -> Result<(), crate::ImportError>,
    ) -> Result<u64, crate::ImportError> {
        self.edit_candidate(
            scanned_key(watched_folder_path, candidate_path),
            &read.content_hash,
            read.file_edit_revision,
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
    /// the loaded value, and the save re-checks them in its transaction. A
    /// metadata expectation left `None` is the loaded value itself: a typed
    /// field is independent of the draft it is typed over, so it pins the
    /// file shape alone.
    async fn edit_candidate(
        &self,
        scanned: ScannedCandidateKey,
        content_hash: &str,
        expected_file_edit_revision: u64,
        expected_metadata_revision: Option<u64>,
        change: impl FnOnce(&mut CandidatePreparation) -> Result<(), crate::ImportError>,
    ) -> Result<u64, crate::ImportError> {
        let mut prep = self
            .load_stored(content_hash)
            .await?
            .ok_or_else(|| CandidateStateRefusal::NoState(content_hash.to_string()))?;
        if !prep.file_edits.is_at_revision(expected_file_edit_revision) {
            return Err(CandidateAsRead::files_moved(content_hash, expected_file_edit_revision));
        }
        if let Some(expected) = expected_metadata_revision {
            if prep.metadata_revision != expected {
                return Err(CandidateAsRead::metadata_moved(content_hash, expected));
            }
        }
        let expected = CandidateSaveExpectation {
            edit_revision: prep.file_edits.revision,
            metadata_revision: prep.metadata_revision,
            scanned: Some(scanned),
        };
        change(&mut prep)?;
        // An edit is the person working on the draft, whoever wrote it before:
        // after it, the draft is their answer rather than what identification
        // or the folder's tags proposed.
        prep.author = crate::MetadataAuthor::Person;
        prep.metadata_revision += 1;
        let revision = prep.metadata_revision;
        match self
            .save_candidate_preparation(prep, expected, CandidateSaveExtras::default())
            .await?
        {
            CandidateSaved::Landed(_) => Ok(revision),
            CandidateSaved::Superseded => {
                Err(CandidateStateRefusal::Superseded(content_hash.to_string()).into())
            }
            CandidateSaved::CoverRefused(refusal) => Err(refusal.into()),
        }
    }
}

/// Why a pane's edit of a draft's tracks was refused. The pane edits the rows
/// of the draft at the revision it read, and the revisions are checked first,
/// so each of these is bae's own fault.
#[derive(Debug, thiserror::Error)]
pub enum TrackEditRefusal {
    /// A fill of the tracks' artists named no track.
    #[error("a track artist fill must name at least one track")]
    NoTrackNamed,
    /// The edit names no row of the draft.
    #[error("{0} is not a row of this draft")]
    NotARow(String),
    /// The edit would change the audio under a row, which is the folder's.
    #[error("the edit of row {0} changes its audio")]
    AudioChanged(String),
}

fn scanned_key(
    watched_folder_path: &std::path::Path,
    candidate_path: &crate::CandidateKey,
) -> ScannedCandidateKey {
    ScannedCandidateKey {
        watched_folder_path: watched_folder_path.to_path_buf(),
        candidate_path: candidate_path.clone(),
    }
}

/// Replace one row's title and artists. Its audio is the folder's, so a row
/// edit that changes it is refused; its place is not in the edit at all.
fn apply_track_edit(
    draft: &mut CandidateDraft,
    row: &crate::RawTrackEdit,
) -> Result<(), crate::ImportError> {
    let track = draft
        .tracks
        .iter_mut()
        .find(|track| track.edit.id == row.id)
        .ok_or_else(|| TrackEditRefusal::NotARow(row.id.clone()))?;
    if row.file.as_ref() != Some(&track.edit.file) {
        return Err(TrackEditRefusal::AudioChanged(row.id.clone()).into());
    }
    track.edit.title.clone_from(&row.title);
    track.edit.artist_assignments.clone_from(&row.artist_assignments);
    Ok(())
}

fn fill_track_artists(
    draft: &mut CandidateDraft,
    track_ids: &[String],
    assignments: &TrackArtistAssignments,
) -> Result<(), crate::ImportError> {
    for track_id in track_ids {
        let track = draft
            .tracks
            .iter_mut()
            .find(|track| &track.edit.id == track_id)
            .ok_or_else(|| TrackEditRefusal::NotARow(track_id.clone()))?;
        track.edit.artist_assignments = assignments.clone();
    }
    Ok(())
}

#[cfg(test)]
#[path = "pane_edits_tests.rs"]
mod tests;
