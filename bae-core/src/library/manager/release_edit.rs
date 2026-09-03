//! Persisted release metadata editor loading and source reset.

use super::release::cover_ref_for;
use super::*;

impl LibraryManager {
    /// Seed the edit form for an existing library release from its current
    /// metadata — the read counterpart to `apply_release_metadata_user_edit`.
    /// Reads the album title and artists, the release pressing fields, and the
    /// per-track titles/sides/numbers/artists, projects them into a wire
    /// `ReleaseUserEdit` describing the current state, then renders that into
    /// the raw editor form via `RawReleaseEdit::from_user_edit`. A track with
    /// no artist rows of its own seeds an empty artist field ("shares the album
    /// artist"); the album artists seed the album artist field.
    pub async fn release_edit_seed(
        &self,
        release_id: &str,
    ) -> Result<crate::import::ReleaseEditSeed, LibraryError> {
        let context = self
            .database
            .find_release_detail_context(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let release = &context.detail.release;
        let album = self
            .database
            .find_album_by_id(&release.album_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Import(format!("Album '{}' not found", release.album_id))
            })?;
        let can_reset_to_source = release.metadata_provenance.is_some();
        let display = crate::album_detail::ReleaseEditDisplayContext::from_raw(&context.detail)?;
        let cover = cover_ref_for(&self.database, release_id).await?;

        let album_artist_assignments: Vec<crate::import::ArtistAssignment> = context
            .album_artists
            .into_iter()
            .map(|artist| crate::import::ArtistAssignment::Existing {
                artist: artist.into(),
            })
            .collect();

        let mut tracks = Vec::with_capacity(context.detail.tracks.len());
        for entry in &context.detail.tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artists: Vec<crate::import::ArtistAssignment> = entry
                .artists
                .clone()
                .into_iter()
                .map(|artist| crate::import::ArtistAssignment::Existing {
                    artist: artist.into(),
                })
                .collect();
            tracks.push((
                entry.track.id.clone(),
                crate::import::TrackUserEdit {
                    title: entry.track.title.clone(),
                    side: entry.track.side,
                    track_number: entry.track.track_number,
                    artist_assignments: if artists.is_empty() {
                        crate::import::TrackArtistAssignments::AlbumArtists
                    } else {
                        crate::import::TrackArtistAssignments::Explicit(artists)
                    },
                    // Re-projecting a release's metadata never re-binds its files;
                    // the audio each track already points at stays as it is.
                    file: None,
                },
            ));
        }

        let edit = raw_release_edit_with_persisted_track_ids(
            crate::import::ReleaseUserEdit {
                album_title: album.title,
                album_artist_assignments,
                album_year: album.year,
                pressing: crate::import::PressingEdit {
                    year: release.pressing.year,
                    format: release.pressing.format.clone(),
                    label: release.pressing.label.clone(),
                    catalog_number: release.pressing.catalog_number.clone(),
                    country: release.pressing.country.clone(),
                    barcode: release.pressing.barcode.clone(),
                },
                tracks: tracks.iter().map(|(_, track)| track.clone()).collect(),
            },
            tracks.iter().map(|(id, _)| id.as_str()),
        )?;

        Ok(crate::import::ReleaseEditSeed {
            edit,
            can_reset_to_source,
            cover,
            display,
        })
    }

    /// Re-project the stored source into the editor's raw form while retaining
    /// the database track IDs the current session addresses.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn reset_release_edit_to_source(
        &self,
        release_id: &str,
    ) -> Result<crate::import::RawReleaseEdit, LibraryError> {
        let edit = self.reset_metadata_to_source(release_id).await?;
        let tracks = self.database.get_tracks_for_release(release_id).await?;
        raw_release_edit_with_persisted_track_ids(
            edit,
            tracks.iter().map(|track| track.id.as_str()),
        )
    }
}

fn raw_release_edit_with_persisted_track_ids<'a>(
    edit: crate::import::ReleaseUserEdit,
    track_ids: impl ExactSizeIterator<Item = &'a str>,
) -> Result<crate::import::RawReleaseEdit, LibraryError> {
    if edit.tracks.len() != track_ids.len() {
        return Err(LibraryError::Internal(format!(
            "release editor source has {} tracks for {} persisted track IDs",
            edit.tracks.len(),
            track_ids.len()
        )));
    }
    let crate::import::ReleaseUserEdit {
        album_title,
        album_artist_assignments,
        album_year,
        pressing,
        tracks,
    } = edit;
    Ok(crate::import::RawReleaseEdit {
        album_title,
        album_artist_assignments,
        album_year: album_year.map(|year| year.to_string()).unwrap_or_default(),
        pressing: crate::import::RawPressingEdit::from_pressing(&pressing),
        tracks: tracks
            .into_iter()
            .zip(track_ids)
            .map(|(track, id)| crate::import::RawTrackEdit::from_user_edit(track, id.to_string()))
            .collect(),
    })
}
