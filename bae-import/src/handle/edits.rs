//! The pane's own writes: the cover, the album fields, and the track rows.
//!
//! Each one is stored the moment the control is used, keyed by the
//! candidate's content hash. Nothing is sent: the tables are
//! device-local, so the per-candidate live query sees the commit and the pane
//! redraws from it.

use super::*;

/// The candidate a pane edit of its draft is composed against: where the scan
/// lists it, and the revisions it stood at when read.
struct DraftEditBase {
    watched_folder_path: std::path::PathBuf,
    candidate_path: crate::CandidateKey,
    candidate: crate::CandidateAsRead,
}

impl ImportServiceHandle {
    /// Record the cover the user chose for this candidate.
    pub async fn set_candidate_cover(
        &self,
        candidate_key: &crate::CandidateKey,
        cover: crate::CoverSelection,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move { this.set_candidate_cover_write(&candidate_key, cover).await })
            .await
    }

    async fn set_candidate_cover_write(
        &self,
        candidate_key: &crate::CandidateKey,
        cover: crate::CoverSelection,
    ) -> Result<(), crate::ImportError> {
        let candidate = self.require_release_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        let revision = self
            .services
            .tables
            .load_import_candidate_state(&hash)
            .await?
            .ok_or_else(|| crate::ImportError::Internal {
                detail: format!("{candidate_key} has no stored candidate state"),
            })?
            .metadata_revision;
        let cover = match cover {
            crate::CoverSelection::Remote(image, source) => {
                let fetched = self.services.remote_images.fetch_required(&image.url).await?;
                crate::cover_art::PreparedCover::Remote { image, source, fetched }
            }
            crate::CoverSelection::Local(file_id) => {
                crate::cover_art::PreparedCover::Local(file_id)
            }
            crate::CoverSelection::Embedded(source_file_id) => {
                crate::cover_art::PreparedCover::Embedded(source_file_id)
            }
        };
        let _commit = self
            .commit_lock_for_revision(
                "set the cover",
                candidate_key,
                &hash,
                candidate.file_edit_revision,
            )
            .await?;
        self.services
            .tables
            .set_prepared_cover(
                &candidate.watched_folder_path,
                &candidate.key(),
                &crate::CandidateAsRead {
                    content_hash: hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                &cover,
            )
            .await?;
        Ok(())
    }

    /// Pick the library album `album_id` for this candidate to join, over
    /// the one its records name, or (`None`) leave it to its records.
    pub async fn set_candidate_album(
        &self,
        candidate_key: &crate::CandidateKey,
        album_id: Option<String>,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            let candidate = this.require_release_candidate(&candidate_key).await?;
            let hash = candidate.files.content_hash();
            let _commit = this
                .commit_lock_for_revision(
                    "pick the album",
                    &candidate_key,
                    &hash,
                    candidate.file_edit_revision,
                )
                .await?;
            Ok(this.services.tables.set_candidate_album_pick(&hash, album_id.as_deref()).await?)
        })
        .await
    }

    /// Record one album-level field the user typed or chose.
    pub async fn set_candidate_edit_field(
        &self,
        candidate_key: &crate::CandidateKey,
        edit: crate::DraftFieldEdit,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(
            async move { this.set_candidate_edit_field_write(&candidate_key, edit).await },
        )
        .await
    }

    async fn set_candidate_edit_field_write(
        &self,
        candidate_key: &crate::CandidateKey,
        edit: crate::DraftFieldEdit,
    ) -> Result<(), crate::ImportError> {
        let candidate = self.require_release_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
                "edit a field",
                candidate_key,
                &hash,
                candidate.file_edit_revision,
            )
            .await?;
        self.services
            .tables
            .set_field_prepared(
                &candidate.watched_folder_path,
                &candidate.key(),
                &hash,
                candidate.file_edit_revision,
                edit,
            )
            .await?;
        Ok(())
    }

    /// Replace the ordered album credits with existing library artists, new
    /// artist seeds, or both. The stored assignments remain typed until import
    /// resolves them, so matching names never imply identity.
    pub async fn set_candidate_album_artists(
        &self,
        candidate_key: &crate::CandidateKey,
        assignments: Vec<crate::Credited>,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.set_candidate_album_artists_write(&candidate_key, assignments).await
        })
        .await
    }

    async fn set_candidate_album_artists_write(
        &self,
        candidate_key: &crate::CandidateKey,
        assignments: Vec<crate::Credited>,
    ) -> Result<(), crate::ImportError> {
        let assignments = crate::Credited::normalized_list(assignments);
        let base = self.draft_edit_base(candidate_key).await?;
        let _commit = self
            .commit_lock_for_revision(
                "set album artists",
                candidate_key,
                &base.candidate.content_hash,
                base.candidate.file_edit_revision,
            )
            .await?;
        self.services
            .tables
            .set_album_artists_prepared(
                &base.watched_folder_path,
                &base.candidate_path,
                &base.candidate,
                &assignments,
            )
            .await?;
        Ok(())
    }

    /// Record one mapping-table row's title and artists as the user left them.
    pub async fn set_candidate_track_edit(
        &self,
        candidate_key: &crate::CandidateKey,
        track: crate::RawTrackEdit,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(
            async move { this.set_candidate_track_edit_write(&candidate_key, track).await },
        )
        .await
    }

    async fn set_candidate_track_edit_write(
        &self,
        candidate_key: &crate::CandidateKey,
        mut track: crate::RawTrackEdit,
    ) -> Result<(), crate::ImportError> {
        track.artist_assignments.normalize();
        let base = self.draft_edit_base(candidate_key).await?;
        let _commit = self
            .commit_lock_for_revision(
                "edit a track",
                candidate_key,
                &base.candidate.content_hash,
                base.candidate.file_edit_revision,
            )
            .await?;
        self.services
            .tables
            .set_track_edit_prepared(
                &base.watched_folder_path,
                &base.candidate_path,
                &base.candidate,
                &track,
            )
            .await?;
        Ok(())
    }

    /// Set the same artist assignments on every named mapping-table row as one
    /// edit, preserving each row's title and audio mapping.
    pub async fn set_candidate_track_artists(
        &self,
        candidate_key: &crate::CandidateKey,
        track_ids: Vec<String>,
        assignments: crate::TrackArtistAssignments,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        self.committed(async move {
            this.set_candidate_track_artists_write(&candidate_key, track_ids, assignments).await
        })
        .await
    }

    async fn set_candidate_track_artists_write(
        &self,
        candidate_key: &crate::CandidateKey,
        track_ids: Vec<String>,
        assignments: crate::TrackArtistAssignments,
    ) -> Result<(), crate::ImportError> {
        let base = self.draft_edit_base(candidate_key).await?;
        let _commit = self
            .commit_lock_for_revision(
                "set track artists",
                candidate_key,
                &base.candidate.content_hash,
                base.candidate.file_edit_revision,
            )
            .await?;
        self.services
            .tables
            .set_track_artists_prepared(
                &base.watched_folder_path,
                &base.candidate_path,
                &base.candidate,
                &track_ids,
                &assignments,
            )
            .await?;
        Ok(())
    }

    /// The candidate at `candidate_key` as an edit of its draft reads it, or
    /// the refusal for one whose stored preparation is not of the files the
    /// scan lists.
    async fn draft_edit_base(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<DraftEditBase, crate::ImportError> {
        let (candidate, preparation) = self.metadata_write_base(candidate_key).await?;
        let hash = candidate.files.content_hash();
        if !preparation.file_edits.is_at_revision(candidate.file_edit_revision) {
            return Err(crate::ImportError::Internal {
                detail: format!("{candidate_key} changed before its edit was prepared"),
            });
        }
        Ok(DraftEditBase {
            candidate_path: candidate.key(),
            watched_folder_path: candidate.watched_folder_path,
            candidate: crate::CandidateAsRead {
                content_hash: hash,
                file_edit_revision: preparation.file_edits.revision,
                metadata_revision: preparation.metadata_revision,
            },
        })
    }
}

#[cfg(test)]
#[path = "edits_tests.rs"]
mod tests;
