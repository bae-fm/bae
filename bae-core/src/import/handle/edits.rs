//! The pane's own writes: the cover, the album fields, and the track rows.
//!
//! Each one is stored the moment the control is used, keyed by the
//! candidate's content hash. Nothing is broadcast: the tables are
//! device-local, so the per-candidate live query sees the commit and the pane
//! redraws from it.

use super::*;

struct PreparedArtistEdit {
    watched_folder_path: String,
    candidate_path: String,
    content_hash: String,
    file_edit_revision: u64,
    metadata_revision: u64,
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
            crate::import::CoverSelection::Remote(url, _) => Some(
                self.library_manager
                    .fetch_required_remote_image(url)
                    .await?,
            ),
            crate::import::CoverSelection::Local(_)
            | crate::import::CoverSelection::Embedded(_) => None,
        };
        self.library_manager
            .save_import_candidate_prepared_cover(
                &candidate.watched_folder_path,
                &candidate.path.to_string_lossy(),
                &hash,
                candidate.file_edit_revision,
                revision,
                &cover,
                remote_image.as_ref(),
            )
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
        let candidate = self.editable_candidate(candidate_key).await?;
        let hash = candidate.files.content_hash();
        self.library_manager
            .save_import_candidate_edit_field_prepared(
                &candidate.watched_folder_path,
                &candidate.path.to_string_lossy(),
                &hash,
                candidate.file_edit_revision,
                field,
                &value,
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
        let replacement = assignments.clone();
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                draft.album_artist_assignments = replacement;
            })
            .await?;
        self.library_manager
            .replace_import_candidate_album_artists_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.content_hash,
                prepared.file_edit_revision,
                prepared.metadata_revision,
                &assignments,
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
            )
            .await?;
        Ok(())
    }

    /// Record one mapping-table row as the user left it.
    ///
    /// Pointing the row at audio another row holds is a swap: the other row
    /// takes this row's previous audio in the same write, so two rows can
    /// never hold one file and the displaced file never silently unbinds.
    pub async fn set_candidate_track_edit(
        &self,
        candidate_key: &str,
        track: crate::import::RawTrackEdit,
    ) -> Result<(), crate::import::ImportError> {
        let replacement = track.clone();
        let mut displaced: Option<crate::import::RawTrackEdit> = None;
        let displaced_out = &mut displaced;
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                let previous_file = draft
                    .tracks
                    .iter()
                    .find(|row| row.id == replacement.id)
                    .and_then(|row| row.file.clone());
                if let Some(new_file) = replacement
                    .file
                    .as_ref()
                    .filter(|f| previous_file.as_ref() != Some(f))
                {
                    if let Some(other) = draft
                        .tracks
                        .iter_mut()
                        .find(|row| row.id != replacement.id && row.file.as_ref() == Some(new_file))
                    {
                        other.file = previous_file;
                        *displaced_out = Some(other.clone());
                    }
                }
                if let Some(current) = draft.tracks.iter_mut().find(|row| row.id == replacement.id)
                {
                    *current = replacement;
                }
            })
            .await?;
        let mut edits = vec![crate::import::CandidateTrackEdit::edited(track)];
        if let Some(displaced) = displaced {
            edits.push(crate::import::CandidateTrackEdit::edited(displaced));
        }
        self.library_manager
            .save_import_candidate_track_edits_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.content_hash,
                prepared.file_edit_revision,
                prepared.metadata_revision,
                &edits,
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
        self.library_manager
            .replace_import_candidate_track_artists_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.content_hash,
                prepared.file_edit_revision,
                prepared.metadata_revision,
                &track_ids,
                &assignments,
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
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
        let dropped_id = track_id.clone();
        let prepared = self
            .prepared_artist_edit(candidate_key, move |draft| {
                draft.tracks.retain(|track| track.id != dropped_id);
            })
            .await?;
        self.library_manager
            .save_import_candidate_track_edits_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.content_hash,
                prepared.file_edit_revision,
                prepared.metadata_revision,
                &[crate::import::CandidateTrackEdit::dropped(track_id)],
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
    ) -> Result<crate::import::folder_scanner::FolderCandidate, crate::import::ImportError> {
        match self.get_candidate(candidate_key).await? {
            Some(super::ImportCandidateSnapshot::Folder {
                candidate,
                actionable: true,
                ..
            }) => Ok(candidate),
            _ => Err(crate::import::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            }),
        }
    }

    async fn prepared_artist_edit(
        &self,
        candidate_key: &str,
        decide: impl FnOnce(&mut crate::import::RawReleaseEdit),
    ) -> Result<PreparedArtistEdit, crate::import::ImportError> {
        let candidate = self.editable_candidate(candidate_key).await?;
        let files = &candidate.files;
        let hash = files.content_hash();
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
        let mut active = crate::import::edits::apply_track_mappings_to_draft(
            preparation.metadata_draft,
            &preparation.track_mappings,
        )?;
        decide(&mut active);
        let (source_discogs_artist_ids, assets) = self
            .prepared_artist_images_for_active(
                candidate_key,
                files,
                preparation.metadata_provenance.as_ref(),
                &active,
                preparation.assets.artist_images,
            )
            .await?;
        Ok(PreparedArtistEdit {
            watched_folder_path: candidate.watched_folder_path,
            candidate_path: candidate.path.to_string_lossy().into_owned(),
            content_hash: hash,
            file_edit_revision: preparation.file_edit_revision,
            metadata_revision: preparation.metadata_revision,
            source_discogs_artist_ids,
            assets,
        })
    }

    pub(super) async fn prepared_artist_images_for_active(
        &self,
        candidate_key: &str,
        files: &crate::import::folder_scanner::CategorizedFiles,
        provenance: Option<&crate::import::MetadataProvenance>,
        active: &crate::import::RawReleaseEdit,
        current: Vec<crate::import::PreparedArtistImage>,
    ) -> Result<
        (
            std::collections::BTreeSet<String>,
            Vec<crate::import::PreparedArtistImage>,
        ),
        crate::import::ImportError,
    > {
        let source_discogs_artist_ids = self
            .source_discogs_artist_ids_for_active_tracks(candidate_key, files, provenance, active)
            .await?;
        let required_discogs_artist_ids = source_discogs_artist_ids
            .union(&active.new_discogs_artist_ids_for_bound_tracks())
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

    async fn source_discogs_artist_ids_for_active_tracks(
        &self,
        candidate_key: &str,
        files: &crate::import::folder_scanner::CategorizedFiles,
        provenance: Option<&crate::import::MetadataProvenance>,
        active: &crate::import::RawReleaseEdit,
    ) -> Result<std::collections::BTreeSet<String>, crate::import::ImportError> {
        let Some(crate::import::MetadataProvenance::ExternalRelease { source, release_id }) =
            provenance
        else {
            return Ok(Default::default());
        };
        let release_ref = crate::import::MetadataRef::new(release_id.clone(), *source);
        let payloads = self
            .library_manager
            .load_release_payloads(&release_ref)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key}'s selected release payloads are not prepared"),
            })?;
        let durations = crate::import::probe::source_durations(files)?;
        let audio_durations = crate::import::track_slots::audio_durations(files, &durations)?;
        let mut parsed =
            payloads.parsed_for_audio(&audio_durations, self.clock.as_ref(), self.ids.as_ref())?;
        crate::import::pane::retain_mapped_source_track_metadata(
            &mut parsed,
            &active.tracks,
            crate::import::pane::CANDIDATE_TRACK_ID_PREFIX,
        );
        Ok(crate::import::pane::source_discogs_artist_ids(&parsed))
    }
}
