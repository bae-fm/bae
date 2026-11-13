//! A user's metadata edit of a release: the rows it leaves the release, and
//! the write a surface asks for directly.

use super::*;

impl LibraryManager {
    /// Load the album id, release, album, and existing tracks for a release
    /// being edited — the shared prelude of `release_edit_seed` and
    /// `apply_release_metadata_user_edit`.
    pub(super) async fn load_release_for_edit(
        &self,
        release_id: &str,
    ) -> Result<(String, DbRelease, DbAlbum, Vec<DbTrack>), LibraryError> {
        let album_id =
            self.records.find_album_id_for_release(release_id).await?.ok_or_else(|| {
                LibraryError::release_missing(release_id, super::ReleaseWork::Edit)
            })?;
        let release =
            self.records.find_release_by_id(release_id).await?.ok_or_else(|| {
                LibraryError::release_missing(release_id, super::ReleaseWork::Edit)
            })?;
        let album = self
            .records
            .find_album_by_id(&album_id)
            .await?
            .ok_or_else(|| LibraryError::AlbumMissing { album_id: album_id.clone() })?;
        let existing_tracks = self.records.get_tracks_for_release(release_id).await?;
        Ok((album_id, release, album, existing_tracks))
    }

    /// Apply a user-supplied metadata edit to an existing release.
    ///
    /// Every surface a user edits through reaches the write here — the desktop
    /// editor and MCP's `release_metadata_update`. The desktop shapes its form
    /// first ([`bae_import::RawReleaseEdit::shape`]); MCP hands over
    /// a wire edit built field-for-field. So the edit is normalized and validated
    /// here rather than at any one caller: a blank album title or an artist-less
    /// album is rejected, and surrounding whitespace is trimmed, no matter which
    /// surface it came from.
    ///
    /// Writes only the rows `release_metadata_rows` describes; see it
    /// for the row-level contract.
    pub async fn apply_release_metadata_user_edit(
        &self,
        release_id: &str,
        edit: &bae_import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        let edit = edit.clone().normalized();
        edit.validate()?;
        let (_, release, album, existing_tracks) = self.load_release_for_edit(release_id).await?;
        let metadata = self.release_metadata_rows(album, release, &existing_tracks, &edit)?;
        self.records.update_release_metadata_user_edit(metadata).await?;
        Ok(())
    }

    /// The rows `edit` leaves `release`, filed under `album`, with: album title
    /// and artists, release pressing fields, and per-track titles and
    /// artists. A picked
    /// library artist is linked as itself; every other credit is resolved by the
    /// artist rule (`bae_records::records::artist_resolution`) inside the commit that writes the
    /// album/release/track rows and replaces the `album_artists` /
    /// `track_artists` junctions.
    ///
    /// Track edits align positionally with the release's existing tracks (the
    /// edit can't add or remove tracks — `tracks.len()` must equal the
    /// release's track count). Album artists and per-track artists are
    /// positional lists — the order in `album_artist_assignments` and each
    /// track's explicit assignments becomes the `position` column on the
    /// `album_artists` / `track_artists` rows.
    ///
    /// The stored catalog release, the release records, and where the draft was
    /// read are deliberately not touched. Identity is orthogonal to metadata;
    /// what the source said stays put.
    ///
    /// Ungated on purpose: a release reseeded from sparse file tags carries a
    /// blank-but-editable title and artist, which the user fills in the editor.
    /// A *user's* edit is held to [`bae_import::ReleaseUserEdit::validate`] —
    /// it arrives through [`Self::apply_release_metadata_user_edit`].
    pub(super) fn release_metadata_rows(
        &self,
        album: DbAlbum,
        release: DbRelease,
        existing_tracks: &[DbTrack],
        edit: &bae_import::ReleaseUserEdit,
    ) -> Result<bae_records::ReleaseMetadataRows, LibraryError> {
        use bae_records::{DbAlbumArtist, DbTrackArtist};

        if existing_tracks.len() != edit.tracks.len() {
            return Err(LibraryError::TrackCountChanged {
                release_tracks: existing_tracks.len(),
                edit_tracks: edit.tracks.len(),
            });
        }

        let now = bae_store::utc_now(self.clock.as_ref());
        let mut credits: Vec<DbArtist> = Vec::new();
        let mut picked: Vec<String> = Vec::new();
        let mut credit_of = |assignment: &bae_import::ArtistAssignment| {
            let credit = assignment.write_row(self.ids.as_ref(), now);
            let id = credit.id.clone();
            if matches!(assignment, bae_import::ArtistAssignment::Picked { .. }) {
                picked.push(id.clone());
            }
            credits.push(credit);
            id
        };

        let album_credit_ids: Vec<String> = edit
            .album_artist_assignments
            .iter()
            .map(|credited| credit_of(&credited.artist))
            .collect();
        if album_credit_ids.is_empty() {
            return Err(LibraryError::Internal(format!(
                "release {} metadata carries no album artist",
                release.id
            )));
        }

        let updated_album = DbAlbum {
            title: edit.album_title.clone(),
            year: edit.album_year,
            is_compilation: bae_import::ArtistAssignment::credit_a_compilation(
                &edit.album_artist_assignments,
            ),
            ..album
        };

        let updated_release = DbRelease { pressing: edit.pressing.clone(), ..release };

        let track_updates: Vec<(String, DbTrack)> = existing_tracks
            .iter()
            .zip(edit.tracks.iter())
            .map(|(existing, t)| {
                let updated = DbTrack { title: t.title.clone(), ..existing.clone() };
                (existing.id.clone(), updated)
            })
            .collect();

        let album_artists: Vec<DbAlbumArtist> = album_credit_ids
            .iter()
            .zip(&edit.album_artist_assignments)
            .zip(0..)
            .map(|((artist_id, credited), position)| DbAlbumArtist {
                credit: credited.artist.link_credit(),
                join_phrase: credited.join.clone(),
                ..DbAlbumArtist::new(self.ids.as_ref(), &updated_album.id, artist_id, position, now)
            })
            .collect();

        let mut track_artists: Vec<DbTrackArtist> = Vec::new();
        for (existing, track) in existing_tracks.iter().zip(edit.tracks.iter()) {
            let bae_import::TrackArtistAssignments::Explicit(assignments) =
                &track.artist_assignments
            else {
                continue;
            };
            for (credited, position) in assignments.iter().zip(0..) {
                track_artists.push(DbTrackArtist {
                    credit: credited.artist.link_credit(),
                    join_phrase: credited.join.clone(),
                    ..DbTrackArtist::new(
                        self.ids.as_ref(),
                        &existing.id,
                        &credit_of(&credited.artist),
                        position,
                        now,
                    )
                });
            }
        }

        Ok(bae_records::ReleaseMetadataRows {
            album: updated_album,
            release: updated_release,
            track_updates,
            credits,
            picked,
            album_artists,
            track_artists,
        })
    }
}

#[cfg(test)]
#[path = "release_metadata_tests.rs"]
mod tests;
