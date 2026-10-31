//! Release domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Every catalog's description of a release. Empty when no catalog
    /// describes it. A read every platform makes: the mobile apps draw a
    /// release's records too, so it lives outside the desktop-only import
    /// operations.
    pub async fn get_release_records(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseRecord>, LibraryError> {
        Ok(self.database.get_release_records(release_id).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_audio_format_with_segments_for_test(
        &self,
        audio_format: &DbAudioFormat,
        segments: &[DbAudioSegment],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_audio_format_with_segments_for_test(audio_format, segments)
            .await?;
        Ok(())
    }

    /// Test-only: seed a single file row, its file staged on this device
    /// only from an original of `size` zero bytes written under the app
    /// directory, for a test that doesn't read its bytes. Production inserts
    /// files only as part of an import.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_file(
        &self,
        file: &crate::db::NewReleaseFile,
        size: u64,
    ) -> Result<coven::FileId, LibraryError> {
        let folder = self.app_dir.test_originals();
        std::fs::create_dir_all(&folder)?;
        let path = folder.join(&file.id);
        std::fs::File::create_new(&path)?.set_len(size)?;
        self.add_external_file_for_test(file, &path).await
    }

    /// Test-only: seed a file row, its file staged on this device only from
    /// the original at `path`, as a local import stages it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_external_file_for_test(
        &self,
        file: &crate::db::NewReleaseFile,
        path: &std::path::Path,
    ) -> Result<coven::FileId, LibraryError> {
        Ok(self.database.insert_file_for_test(file, path).await?)
    }

    pub async fn get_release_by_id(
        &self,
        release_id: &str,
    ) -> Result<Option<DbRelease>, LibraryError> {
        Ok(self.database.find_release_by_id(release_id).await?)
    }

    /// Whether this device imported a release from files whose folder hash is
    /// `hash`. The import watcher stamps each scanned candidate with this, so
    /// a folder imported here is Done after a restart, and after a move or a
    /// rename: it matches by file structure, not by path.
    #[cfg(desktop)]
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, LibraryError> {
        Ok(self.release_imported_from(hash).await?.is_some())
    }

    /// The release this device imported from files whose folder hash is
    /// `hash`, if it imported one.
    #[cfg(desktop)]
    pub async fn release_imported_from(&self, hash: &str) -> Result<Option<String>, LibraryError> {
        Ok(self.database.release_imported_from(hash).await?)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }

    /// A release's mediums, in order, which its tracks' places read against.
    pub async fn get_release_mediums(
        &self,
        release_id: &str,
    ) -> Result<crate::db::DbReleaseMediums, LibraryError> {
        Ok(self.database.get_release_mediums(release_id).await?)
    }

    /// The library album an import carrying `records` joins, or `None` for a
    /// new one (see [`crate::db::Database::album_records_name`]).
    #[cfg(desktop)]
    pub async fn find_existing_album_for_import(
        &self,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<Option<String>, crate::import::ImportError> {
        self.database
            .album_records_name(records, &[])
            .await
            .map_err(|e| crate::import::ImportError::Db(LibraryError::Database(e)))
    }

    /// The audio a catalog release is read against for the library release
    /// `release_id`: its tracks' stored lengths, and what its own files say
    /// it was copied from.
    #[cfg(desktop)]
    pub(super) async fn release_read_audio(
        &self,
        release_id: &str,
    ) -> Result<crate::import::catalog_reading::ReadAudio, LibraryError> {
        crate::signals::release::resolve_release_identity(self, release_id)
            .await?
            .read_audio()
            .map_err(LibraryError::from)
    }

    /// Test-only: seed a full album/release/tracks in one call. Production imports
    /// go through `finalize_import_atomic`.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        album_artists: &[crate::db::DbAlbumArtist],
        release: &DbRelease,
        mediums: &[crate::db::DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_album_with_release_and_tracks(
                album,
                album_artists,
                release,
                mediums,
                tracks,
                track_artists,
            )
            .await?;
        Ok(())
    }

    /// Test-only: seed a second release onto an already-inserted album.
    #[cfg(test)]
    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        mediums: &[crate::db::DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_release_with_tracks(release, mediums, tracks, track_artists)
            .await?;
        Ok(())
    }

    /// Load the album id, release, album, and existing tracks for a release
    /// being edited — the shared prelude of `release_edit_seed` and
    /// `apply_release_metadata_user_edit`.
    pub(super) async fn load_release_for_edit(
        &self,
        release_id: &str,
    ) -> Result<(String, DbRelease, DbAlbum, Vec<DbTrack>), LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Refused(format!("Release '{release_id}' not found")))?;
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Refused(format!("Release '{release_id}' not found")))?;
        let album = self
            .database
            .find_album_by_id(&album_id)
            .await?
            .ok_or_else(|| LibraryError::Refused(format!("Album '{album_id}' not found")))?;
        let existing_tracks = self.database.get_tracks_for_release(release_id).await?;
        Ok((album_id, release, album, existing_tracks))
    }

    /// Apply a user-supplied metadata edit to an existing release.
    ///
    /// Every surface a user edits through reaches the write here — the desktop
    /// editor and MCP's `release_metadata_update`. The desktop shapes its form
    /// first ([`crate::import::RawReleaseEdit::shape`]); MCP hands over
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
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        let edit = edit.clone().normalized();
        edit.validate()?;
        let _writing = self.release_writes.lock().await;
        let (_, release, album, existing_tracks) = self.load_release_for_edit(release_id).await?;
        let metadata = self.release_metadata_rows(album, release, &existing_tracks, &edit)?;
        self.database
            .update_release_metadata_user_edit(metadata)
            .await?;
        Ok(())
    }

    /// The rows `edit` leaves `release`, filed under `album`, with: album title
    /// and artists, release pressing fields, and per-track titles and
    /// artists. A picked
    /// library artist is linked as itself; every other credit is resolved by the
    /// artist rule (`db::artist_resolution`) inside the commit that writes the
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
    /// A *user's* edit is held to [`crate::import::ReleaseUserEdit::validate`] —
    /// it arrives through [`Self::apply_release_metadata_user_edit`].
    pub(super) fn release_metadata_rows(
        &self,
        album: DbAlbum,
        release: DbRelease,
        existing_tracks: &[DbTrack],
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<crate::db::ReleaseMetadataRows, LibraryError> {
        use crate::db::{DbAlbumArtist, DbTrackArtist};

        if existing_tracks.len() != edit.tracks.len() {
            return Err(LibraryError::Refused(format!(
                "Track count mismatch: release has {} tracks, edit supplies {}",
                existing_tracks.len(),
                edit.tracks.len()
            )));
        }

        let now = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        let mut credits: Vec<DbArtist> = Vec::new();
        let mut picked: Vec<String> = Vec::new();
        let mut credit_of = |assignment: &crate::import::ArtistAssignment| {
            let credit = assignment.write_row(self.ids.as_ref(), now);
            let id = credit.id.clone();
            if matches!(assignment, crate::import::ArtistAssignment::Picked { .. }) {
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
            is_compilation: crate::import::ArtistAssignment::credit_a_compilation(
                &edit.album_artist_assignments,
            ),
            ..album
        };

        let updated_release = DbRelease {
            pressing: edit.pressing.clone(),
            ..release
        };

        let track_updates: Vec<(String, DbTrack)> = existing_tracks
            .iter()
            .zip(edit.tracks.iter())
            .map(|(existing, t)| {
                let updated = DbTrack {
                    title: t.title.clone(),
                    ..existing.clone()
                };
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
                ..DbAlbumArtist::new(&updated_album.id, artist_id, position, now)
            })
            .collect();

        let mut track_artists: Vec<DbTrackArtist> = Vec::new();
        for (existing, track) in existing_tracks.iter().zip(edit.tracks.iter()) {
            let crate::import::TrackArtistAssignments::Explicit(assignments) =
                &track.artist_assignments
            else {
                continue;
            };
            for (credited, position) in assignments.iter().zip(0..) {
                track_artists.push(DbTrackArtist {
                    credit: credited.artist.link_credit(),
                    join_phrase: credited.join.clone(),
                    ..DbTrackArtist::new(&existing.id, &credit_of(&credited.artist), position, now)
                });
            }
        }

        Ok(crate::db::ReleaseMetadataRows {
            album: updated_album,
            release: updated_release,
            track_updates,
            credits,
            picked,
            album_artists,
            track_artists,
        })
    }

    /// The storage summary for one release, or `None` if it doesn't exist. The
    /// download queue reads it at enqueue time to skip a local or
    /// already-pinned release.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseStorageSummary>, LibraryError> {
        let Some(raw) = self
            .database
            .find_release_storage_summary(release_id)
            .await?
        else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let originals = self
            .look_at_originals(crate::album_detail::recorded_originals(&raw.unstored_audio))
            .await?;
        Ok(Some(ReleaseStorageSummary::from_raw(
            raw,
            has_cloud_home,
            &originals,
        )))
    }

    pub(crate) fn subscribe_release_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, crate::db::ReleaseDetailProjection> {
        self.database.subscribe_release_detail(initial)
    }

    /// The originals the library records this device holding for the release
    /// a release-detail delivery shows: what the disk is asked about.
    pub(crate) fn release_detail_originals(
        projection: &crate::db::ReleaseDetailProjection,
    ) -> Vec<crate::db::RecordedOriginal> {
        projection
            .context
            .as_ref()
            .map(|context| crate::album_detail::recorded_originals(&context.detail.unstored_audio))
            .unwrap_or_default()
    }

    /// Resolve a release-detail delivery with what the disk has said of its
    /// originals (see [`Self::release_detail_originals`]).
    pub(crate) fn resolve_release_detail_projection(
        &self,
        release_id: &str,
        projection: crate::db::ReleaseDetailProjection,
        originals: &crate::album_detail::OriginalsSeen,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some(crate::db::ReleaseDetailContext {
            detail,
            album_artists,
            release_index,
            is_compilation,
        }) = projection.context
        else {
            return Ok(None);
        };
        Ok(Some(self.resolve_release_detail_context(
            release_id,
            detail,
            album_artists,
            release_index,
            is_compilation,
            originals,
        )))
    }

    fn resolve_release_detail_context(
        &self,
        release_id: &str,
        raw: crate::db::DbReleaseDetail,
        album_artists: Vec<crate::credits::LibraryCredit>,
        release_index: usize,
        is_compilation: bool,
        originals: &crate::album_detail::OriginalsSeen,
    ) -> ReleaseDetail {
        let has_cloud_home = self.has_cloud_home();
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            originals,
            cover: crate::album_detail::cover_ref(release_id, raw.cover_version.clone()),
            transfer_action: self.current_transfer_action(release_id),
            is_compilation,
        };
        let (detail, orphans) = ReleaseDetail::from_raw(raw, &album_artists, release_index, &ctx);
        self.report_audio_format_orphans(orphans);
        detail
    }

    /// Resolved release detail for the album-detail view: a `ReleaseSummary` plus
    /// the tracks/files/gallery its SQL joins load, and the release's position
    /// within its album, so `display_name` needs no index from the caller. `None`
    /// when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some(crate::db::ReleaseDetailContext {
            detail: raw,
            album_artists,
            release_index,
            is_compilation,
        }) = self
            .database
            .find_release_detail_context(release_id)
            .await?
        else {
            return Ok(None);
        };
        let originals = self
            .look_at_originals(crate::album_detail::recorded_originals(&raw.unstored_audio))
            .await?;
        Ok(Some(self.resolve_release_detail_context(
            release_id,
            raw,
            album_artists,
            release_index,
            is_compilation,
            &originals,
        )))
    }

    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.database.get_releases_for_album(album_id).await?)
    }

    pub async fn check_releases_in_library(
        &self,
        checks: &[crate::db::LibraryCheck],
    ) -> Result<Vec<crate::db::LibraryStatus>, LibraryError> {
        Ok(self.database.check_releases_in_library(checks).await?)
    }

    pub(crate) fn subscribe_library_statuses(
        &self,
        initial: std::collections::BTreeSet<crate::db::LibraryCheck>,
    ) -> coven::ReconfigurableLiveQuery<
        std::collections::BTreeSet<crate::db::LibraryCheck>,
        Vec<crate::db::LibraryStatus>,
    > {
        self.database.subscribe_library_statuses(initial)
    }

    /// Every file of a release — audio files, and the metadata files (cover art,
    /// CUE sheets) that no track owns. Files belong to releases, not to albums or
    /// tracks; a track reaches its audio file through its `audio_segments`.
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.database.get_files_for_release(release_id).await?)
    }

    /// The release's files in their order, each with the path of the original
    /// this device holds for it; `None` where it holds none.
    #[cfg(desktop)]
    pub(crate) async fn get_files_with_originals_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<(DbFile, Option<std::path::PathBuf>)>, LibraryError> {
        Ok(self
            .database
            .get_files_with_originals_for_release(release_id)
            .await?)
    }

    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }

    /// Set which of an album's releases provides its cover art.
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .set_album_primary_release(album_id, primary_release_id)
            .await?;

        Ok(())
    }

    /// Delete a release and its data. The rows go in one transaction, taking the
    /// album with them if this was its last release. Coven owns retention of
    /// files still referenced by committed uploads or shared history.
    pub async fn delete_release(&self, release_id: &str) -> Result<(), LibraryError> {
        self.database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("Release not found: {release_id}"))
            })?;

        // Read the track ids before the delete cascades them away — playback needs
        // them to clear the queue.
        let track_ids: Vec<String> = self
            .get_tracks_for_release(release_id)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        let deletion = self.database.plan_release_deletion(release_id).await?;
        // Deletion is a local transaction. Each coven file of the release goes
        // with its last reference, which takes its upload off the queue.
        self.database.delete_release_with_cleanup(deletion).await?;

        if !track_ids.is_empty() {
            self.tracks_deleted(track_ids);
        }

        Ok(())
    }
}

/// Each of a library release's tracks' stored length, in track order.
#[cfg(desktop)]
pub(crate) fn stored_track_lengths(tracks: &[DbTrack]) -> Vec<u64> {
    tracks.iter().map(|track| track.duration_ms).collect()
}

/// The cover [`ImageRef`] for one release from its cover's version, or `None`
/// when it has no cover row. Free function so the manager's `cover_ref`,
/// the release-edit context, and one track's detail share one construction.
pub(crate) async fn cover_ref_for(
    database: &Database,
    release_id: &str,
) -> Result<Option<ImageRef>, LibraryError> {
    Ok(database
        .cover_version(release_id)
        .await?
        .map(|version| ImageRef {
            id: release_id.to_string(),
            version,
            image_type: LibraryImageType::Cover,
        }))
}
