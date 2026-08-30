//! The pane's own writes: the cover, the album fields, and the track rows.
//!
//! Each one is stored the moment the control is used, keyed by the
//! candidate's content hash. Nothing is broadcast: the tables are
//! device-local, so the per-candidate live query sees the commit and the pane
//! redraws from it.

use super::*;

impl ImportServiceHandle {
    /// Record the cover the user chose for this candidate.
    pub async fn set_candidate_cover(
        &self,
        candidate_key: &str,
        cover: crate::import::CoverSelection,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .save_import_candidate_cover(&hash, &cover)
            .await?;
        Ok(())
    }

    /// Record one album-level field the user typed.
    pub async fn set_candidate_edit_field(
        &self,
        candidate_key: &str,
        field: crate::import::CandidateEditField,
        value: String,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .save_import_candidate_edit_field(&hash, field, &value)
            .await?;
        Ok(())
    }

    /// Replace the ordered album credits with existing library artists, new
    /// artist seeds, or both. The stored assignments remain typed until import
    /// resolves them, so matching names never imply identity.
    pub async fn set_candidate_album_artists(
        &self,
        candidate_key: &str,
        assignments: Vec<crate::import::ArtistAssignment>,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .replace_import_candidate_album_artists(&hash, &assignments)
            .await?;
        Ok(())
    }

    /// Record one mapping-table row as the user left it.
    pub async fn set_candidate_track_edit(
        &self,
        candidate_key: &str,
        track: crate::import::RawTrackEdit,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .save_import_candidate_track_edit(
                &hash,
                &crate::import::CandidateTrackEdit::edited(track),
            )
            .await?;
        Ok(())
    }

    /// Set the same artist assignments on every named mapping-table row as one
    /// edit, preserving each row's title and audio mapping.
    pub async fn set_candidate_track_artists(
        &self,
        candidate_key: &str,
        track_ids: Vec<String>,
        assignments: crate::import::TrackArtistAssignments,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .replace_import_candidate_track_artists(&hash, &track_ids, &assignments)
            .await?;
        Ok(())
    }

    /// Take one mapping-table row out of the import: the release commits
    /// without that track. Nothing on disk changes.
    pub async fn drop_candidate_track(
        &self,
        candidate_key: &str,
        track_id: String,
    ) -> Result<(), crate::import::ImportError> {
        let hash = self.edited_candidate_hash(candidate_key).await?;
        self.library_manager
            .save_import_candidate_track_edit(
                &hash,
                &crate::import::CandidateTrackEdit::dropped(track_id),
            )
            .await?;
        Ok(())
    }

    /// The content hash a pane edit is stored under, or the refusal for a key
    /// that names no editable folder.
    async fn edited_candidate_hash(
        &self,
        candidate_key: &str,
    ) -> Result<String, crate::import::ImportError> {
        let Some((files, _)) = self.actionable_candidate_files(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        Ok(files.content_hash())
    }
}
