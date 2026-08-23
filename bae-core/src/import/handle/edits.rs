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

    /// Open the named audio units and store what they play for.
    ///
    /// Run by the selected candidate's own query when identification has not
    /// measured them — a refused verdict, or a folder picked before the sweep
    /// reached it. This is the one place a pane read opens a file, and it
    /// writes what it read, so the next projection asks for nothing.
    ///
    /// A unit that will not open is written with no length rather than left
    /// out: the row is what ends the asking.
    pub async fn probe_candidate_durations(
        &self,
        candidate_key: &str,
        units: Vec<crate::import::AudioFile>,
    ) -> Result<(), crate::import::ImportError> {
        let Some((files, _)) = self.actionable_candidate_files(candidate_key).await? else {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let hash = files.content_hash();
        let probed =
            tokio::task::spawn_blocking(move || crate::import::probe::probe_units(&files, &units))
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("duration probe task failed: {e}"),
                })?;
        self.library_manager
            .save_import_candidate_durations(&hash, &probed)
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
