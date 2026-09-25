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
        let hash = candidate.files().content_hash();
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
        let _commit = self
            .commit_lock_for_revision(candidate_key, &hash, candidate.file_edit_revision())
            .await?;
        self.preparations
            .set_prepared_cover(
                candidate.watched_folder_path(),
                &candidate.key(),
                &crate::import::CandidateAsRead {
                    content_hash: hash,
                    file_edit_revision: candidate.file_edit_revision(),
                    metadata_revision: revision,
                },
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
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.set_candidate_edit_field_write(&candidate_key, field, value)
                .await
        })
        .await
    }

    async fn set_candidate_edit_field_write(
        &self,
        candidate_key: &str,
        field: crate::import::CandidateEditField,
        value: String,
    ) -> Result<(), crate::import::ImportError> {
        let candidate = self.editable_candidate(candidate_key).await?;
        let hash = candidate.files().content_hash();
        let _commit = self
            .commit_lock_for_revision(candidate_key, &hash, candidate.file_edit_revision())
            .await?;
        self.preparations
            .set_field_prepared(
                candidate.watched_folder_path(),
                &candidate.key(),
                &hash,
                candidate.file_edit_revision(),
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
        let _commit = self
            .commit_lock_for_revision(
                candidate_key,
                &prepared.candidate.content_hash,
                prepared.candidate.file_edit_revision,
            )
            .await?;
        self.preparations
            .set_track_edits_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.candidate,
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

    /// Take one mapping-table row out of the import: the release commits
    /// without that track. Nothing on disk changes.
    pub async fn drop_candidate_track(
        &self,
        candidate_key: &str,
        track_id: String,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.drop_candidate_track_write(&candidate_key, track_id)
                .await
        })
        .await
    }

    async fn drop_candidate_track_write(
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
        let _commit = self
            .commit_lock_for_revision(
                candidate_key,
                &prepared.candidate.content_hash,
                prepared.candidate.file_edit_revision,
            )
            .await?;
        self.preparations
            .set_track_edits_prepared(
                &prepared.watched_folder_path,
                &prepared.candidate_path,
                &prepared.candidate,
                &[crate::import::CandidateTrackEdit::dropped(track_id)],
                &prepared.source_discogs_artist_ids,
                &prepared.assets,
            )
            .await?;
        Ok(())
    }

    /// Include one currently available audio source as a newly initialized row.
    /// The rendered offer pins the source configuration and the draft it was
    /// offered beside; adding audio never reapplies the selected release.
    pub async fn add_candidate_track(
        &self,
        candidate_key: &str,
        audio: crate::import::AudioFile,
        read: crate::import::CandidateAsRead,
    ) -> Result<(), crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.add_candidate_track_write(&candidate_key, audio, read)
                .await
        })
        .await
    }

    async fn add_candidate_track_write(
        &self,
        candidate_key: &str,
        audio: crate::import::AudioFile,
        read: crate::import::CandidateAsRead,
    ) -> Result<(), crate::import::ImportError> {
        let (candidate, preparation, available, source_position) = {
            let _commit = self
                .commit_lock_for_revision(
                    candidate_key,
                    &read.content_hash,
                    read.file_edit_revision,
                )
                .await?;
            let candidate = self.editable_candidate(candidate_key).await?;
            let preparation = self
                .library_manager
                .load_import_candidate_preparation(&read.content_hash)
                .await?
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} has no stored import preparation"),
                })?;
            if preparation.file_edit_revision != read.file_edit_revision {
                return Err(crate::import::CandidateAsRead::files_moved(
                    read.file_edit_revision,
                    crate::import::preparation::CandidateWrite::PaneEdit,
                )
                .into());
            }
            let available = crate::import::track_slots::audio_units(candidate.files());
            let source_position = available
                .iter()
                .position(|unit| unit == &audio)
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("{audio:?} is not available in {candidate_key}"),
                })?;
            if preparation
                .draft
                .tracks
                .iter()
                .any(|track| track.edit.file == audio)
            {
                debug!(
                    ?audio,
                    candidate_key, "audio is already included; nothing to write"
                );
                return Ok(());
            }
            if preparation.metadata_revision != read.metadata_revision {
                return Err(
                    crate::import::CandidateAsRead::metadata_moved(read.metadata_revision).into(),
                );
            }
            (candidate, preparation, available, source_position)
        };

        // Snapshot extraction and provider preparation run without the commit
        // lock. The final locked write refuses either revision moving meanwhile.
        let initialized = if self
            .library_manager
            .get_config()
            .prefs
            .prefill_with_file_metadata
        {
            let (snapshot_candidate, snapshot) = self.file_tag_snapshot(candidate_key).await?;
            if snapshot_candidate.files().content_hash() != read.content_hash
                || snapshot_candidate.file_edit_revision() != read.file_edit_revision
            {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("{candidate_key} changed before its audio could be added"),
                });
            }
            let durations = crate::import::probe::source_durations(snapshot_candidate.files())?;
            crate::import::file_metadata_seed::FileMetadataSeed::project(
                &snapshot_candidate,
                snapshot,
                &durations,
                None,
                self.clock.as_ref(),
                self.ids.as_ref(),
            )?
            .draft
        } else {
            candidate.blank_source().draft
        };
        let mut track = initialized
            .tracks
            .into_iter()
            .find(|track| track.edit.file == audio)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("source initialization did not produce {audio:?}"),
            })?;
        track.edit.id = self.ids.new_id();
        track.source_index = None;
        let mut draft = preparation.draft;
        let mut insertion = draft.tracks.len();
        for (index, included) in draft.tracks.iter().enumerate() {
            let position = available
                .iter()
                .position(|unit| unit == &included.edit.file)
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!("included track {} has unavailable audio", included.edit.id),
                })?;
            if position > source_position {
                insertion = index;
                break;
            }
        }
        draft.tracks.insert(insertion, track.clone());
        let (source_discogs_artist_ids, assets) = self
            .prepared_artist_images_for_active(
                preparation.assets.applied_source.as_ref(),
                &draft.release_edit(),
                &draft.tracks,
                preparation.assets.artist_images,
            )
            .await?;
        // Unchanged source revisions preserve the exact available-audio set
        // checked above, including every CUE FILE association and slice index.
        let _commit = self
            .commit_lock_for_revision(candidate_key, &read.content_hash, read.file_edit_revision)
            .await?;
        self.preparations
            .add_track_prepared(
                candidate.watched_folder_path(),
                candidate_key,
                &read,
                &track,
                insertion,
                &source_discogs_artist_ids,
                &assets,
            )
            .await?;
        Ok(())
    }

    /// The scanned candidate revision a pane edit is based on, or the refusal
    /// for a key that names no editable folder.
    async fn editable_candidate(
        &self,
        candidate_key: &str,
    ) -> Result<crate::import::release_candidate::ReleaseCandidate, crate::import::ImportError>
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
        let files = candidate.files();
        let hash = files.content_hash();
        let preparation = self
            .library_manager
            .load_import_candidate_preparation(&hash)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("{candidate_key} has no stored import preparation"),
            })?;
        if preparation.file_edit_revision != candidate.file_edit_revision() {
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
            watched_folder_path: candidate.watched_folder_path().to_string(),
            candidate_path: candidate.key().into_owned(),
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
            .source_discogs_artist_ids_for_active_tracks(source, active, tracks)
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
        source: Option<&crate::import::source_release::AppliedSource>,
        active: &crate::import::RawReleaseEdit,
        tracks: &[crate::import::CandidateTrack],
    ) -> Result<std::collections::BTreeSet<String>, crate::import::ImportError> {
        let Some(source) = source else {
            return Ok(Default::default());
        };
        let mut parsed = source.parsed(self.clock.as_ref(), self.ids.as_ref())?;
        let retained = tracks
            .iter()
            .filter(|track| active.tracks.iter().any(|row| row.id == track.edit.id))
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
