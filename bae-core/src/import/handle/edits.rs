//! The pane's own writes: the cover, the album fields, and the track rows.
//!
//! Each one is stored the moment the control is used, keyed by the
//! candidate's content hash. Nothing is sent: the tables are
//! device-local, so the per-candidate live query sees the commit and the pane
//! redraws from it.

use super::*;

struct PreparedArtistEdit {
    watched_folder_path: String,
    candidate_path: String,
    candidate: crate::import::CandidateAsRead,
    source_discogs_artist_ids: std::collections::BTreeSet<String>,
    assets: Vec<crate::import::PreparedArtistImage>,
}

impl ImportServiceHandle {
    /// Record the cover the user chose for this candidate.
    pub async fn set_candidate_cover(
        &self,
        candidate_key: &str,
        cover: crate::import::CoverSelection,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move { this.set_candidate_cover_write(&candidate_key, cover).await })
            .await
    }

    async fn set_candidate_cover_write(
        &self,
        candidate_key: &str,
        cover: crate::import::CoverSelection,
    ) -> Result<(), crate::import::ImportError> {
        let candidate = self.editable_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        let revision = self
            .library_manager
            .load_import_candidate_state(&hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored candidate state"),
            })?
            .metadata_revision;
        let remote_image = match &cover {
            crate::import::CoverSelection::Remote(image, _) => Some(
                self.library_manager
                    .fetch_required_remote_image(&image.url)
                    .await?,
            ),
            crate::import::CoverSelection::Local(_)
            | crate::import::CoverSelection::Embedded(_) => None,
        };
        let _commit = self
            .commit_lock_for_revision(
"set the cover",candidate_key, &hash, candidate.file_edit_revision)
            .await?;
        self.preparations
            .set_prepared_cover(
                &candidate.watched_folder_path,
                &candidate.key(),
                &crate::import::CandidateAsRead {
                    content_hash: hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                &cover,
                remote_image.as_ref(),
            )
            .await?;
        Ok(())
    }

    /// Record one album-level field the user typed or chose.
    pub async fn set_candidate_edit_field(
        &self,
        candidate_key: &str,
        edit: crate::import::DraftFieldEdit,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_edit_field_write(&candidate_key, edit)
                .await
        })
        .await
    }

    async fn set_candidate_edit_field_write(
        &self,
        candidate_key: &str,
        edit: crate::import::DraftFieldEdit,
    ) -> Result<(), crate::import::ImportError> {
        let candidate = self.editable_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        let _commit = self
            .commit_lock_for_revision(
"edit a field",candidate_key, &hash, candidate.file_edit_revision)
            .await?;
        self.preparations
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
        candidate_key: &str,
        assignments: Vec<crate::import::ArtistAssignment>,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_album_artists_write(&candidate_key, assignments)
                .await
        })
        .await
    }

    async fn set_candidate_album_artists_write(
        &self,
        candidate_key: &str,
        assignments: Vec<crate::import::ArtistAssignment>,
    ) -> Result<(), crate::import::ImportError> {
        let replacement = assignments.clone();
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                draft.album_artist_assignments = replacement;
            })
            .await?;
        let _commit = self
            .commit_lock_for_revision(
"set album artists",
                candidate_key,
                &prepared.candidate.content_hash,
                prepared.candidate.file_edit_revision,
            )
            .await?;
        self.preparations
            .set_album_artists_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.candidate,
                &assignments,
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
            )
            .await?;
        Ok(())
    }

    /// Record one mapping-table row's title and artists as the user left them.
    pub async fn set_candidate_track_edit(
        &self,
        candidate_key: &str,
        track: crate::import::RawTrackEdit,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_track_edit_write(&candidate_key, track)
                .await
        })
        .await
    }

    async fn set_candidate_track_edit_write(
        &self,
        candidate_key: &str,
        track: crate::import::RawTrackEdit,
    ) -> Result<(), crate::import::ImportError> {
        let replacement = track.clone();
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                if let Some(current) = draft.tracks.iter_mut().find(|row| row.id == replacement.id)
                {
                    *current = replacement;
                }
            })
            .await?;
        let _commit = self
            .commit_lock_for_revision(
"edit a track",
                candidate_key,
                &prepared.candidate.content_hash,
                prepared.candidate.file_edit_revision,
            )
            .await?;
        self.preparations
            .set_track_edit_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.candidate,
                &track,
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
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
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_track_artists_write(&candidate_key, track_ids, assignments)
                .await
        })
        .await
    }

    async fn set_candidate_track_artists_write(
        &self,
        candidate_key: &str,
        track_ids: Vec<String>,
        assignments: crate::import::TrackArtistAssignments,
    ) -> Result<(), crate::import::ImportError> {
        let edited_ids = track_ids.clone();
        let replacement = assignments.clone();
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                for track in &mut draft.tracks {
                    if edited_ids.contains(&track.id) {
                        track.artist_assignments = replacement.clone();
                    }
                }
            })
            .await?;
        let _commit = self
            .commit_lock_for_revision(
"set track artists",
                candidate_key,
                &prepared.candidate.content_hash,
                prepared.candidate.file_edit_revision,
            )
            .await?;
        self.preparations
            .set_track_artists_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.candidate,
                &track_ids,
                &assignments,
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
            )
            .await?;
        Ok(())
    }

    /// The scanned candidate revision a pane edit is based on, or the refusal
    /// for a key that names no editable folder.
    async fn editable_candidate(
        &self,
        candidate_key: &str,
    ) -> Result<crate::import::folder_scanner::FolderCandidate, crate::import::ImportError>
    {
        self.get_release_candidate(candidate_key)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable candidate"),
            })
    }

    async fn prepared_artist_edit(
        &self,
        candidate_key: &str,
        decide: impl FnOnce(&mut crate::import::RawReleaseEdit),
    ) -> Result<PreparedArtistEdit, crate::import::ImportError> {
        let candidate = self.editable_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        let preparation = self
            .library_manager
            .load_import_candidate_preparation(&hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        if preparation.file_edit_revision != candidate.file_edit_revision {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} changed before its edit was prepared"),
            });
        }
        let mut active = preparation.draft.release_edit();
        decide(&mut active);
        let (source_discogs_artist_ids, assets) = self
            .prepared_artist_images_for_active(
                preparation.assets.applied_source.as_ref(),
                &active,
                &preparation.draft.tracks,
                preparation.assets.artist_images,
            )
            .await?;
        Ok(PreparedArtistEdit {
            candidate_path: candidate.key(),
            watched_folder_path: candidate.watched_folder_path,
            candidate: crate::import::CandidateAsRead {
                content_hash: hash,
                file_edit_revision: preparation.file_edit_revision,
                metadata_revision: preparation.metadata_revision,
            },
            source_discogs_artist_ids,
            assets,
        })
    }

    pub(super) async fn prepared_artist_images_for_active(
        &self,
        source: Option<&crate::import::source_release::AppliedSource>,
        active: &crate::import::RawReleaseEdit,
        tracks: &[crate::import::CandidateTrack],
        current: Vec<crate::import::PreparedArtistImage>,
    ) -> Result<
        (
            std::collections::BTreeSet<String>,
            Vec<crate::import::PreparedArtistImage>,
        ),
        crate::import::ImportError,
    > {
        let source_discogs_artist_ids = self
            .source_discogs_artist_ids_for_tracks(source, tracks)
            .await?;
        let required_discogs_artist_ids = source_discogs_artist_ids
            .union(&active.credit_discogs_artist_ids_for_bound_tracks())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let prepared_discogs_artist_ids = current
            .iter()
            .map(|asset| asset.discogs_artist_id().to_string())
            .collect();
        let assets = if required_discogs_artist_ids == prepared_discogs_artist_ids {
            current
        } else {
            self.library_manager
                .prepare_discogs_artist_images(required_discogs_artist_ids)
                .await?
        };
        Ok((source_discogs_artist_ids, assets))
    }

    /// The Discogs artists the applied source credits on the tracks the draft
    /// still takes from it.
    async fn source_discogs_artist_ids_for_tracks(
        &self,
        source: Option<&crate::import::source_release::AppliedSource>,
        tracks: &[crate::import::CandidateTrack],
    ) -> Result<std::collections::BTreeSet<String>, crate::import::ImportError> {
        let Some(source) = source else {
            return Ok(Default::default());
        };
        let mut parsed = source.parsed(self.clock.as_ref(), self.ids.as_ref())?;
        let retained = tracks
            .iter()
            .filter_map(|track| track.source_index)
            .map(|index| {
                parsed
                    .tracks
                    .get(index as usize)
                    .map(|track| track.id.clone())
                    .ok_or_else(|| crate::import::ImportError::Internal {
                        detail: format!("draft names unavailable source track {index}"),
                    })
            })
            .collect::<Result<std::collections::HashSet<_>, _>>()?;
        crate::import::service::retain_track_metadata(&mut parsed, &retained);
        Ok(crate::import::pane::source_discogs_artist_ids(&parsed))
    }
}
