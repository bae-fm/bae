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
    ) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, LibraryError> {
        Ok(self.records.get_release_records(release_id).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_audio_format_with_segments_for_test(
        &self,
        audio_format: &bae_records::DbAudioFormat,
        segments: &[bae_records::DbAudioSegment],
    ) -> Result<(), LibraryError> {
        self.records.insert_audio_format_with_segments_for_test(audio_format, segments).await?;
        Ok(())
    }

    /// Test-only: seed a single file row, its file staged on this device
    /// only from an original of `size` zero bytes written under the app
    /// directory, for a test that doesn't read its bytes. Production inserts
    /// files only as part of an import.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_file(
        &self,
        file: &bae_records::NewReleaseFile,
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
        file: &bae_records::NewReleaseFile,
        path: &std::path::Path,
    ) -> Result<coven::FileId, LibraryError> {
        Ok(self.records.insert_file_for_test(file, path).await?)
    }

    pub async fn get_release_by_id(
        &self,
        release_id: &str,
    ) -> Result<Option<DbRelease>, LibraryError> {
        Ok(self.records.find_release_by_id(release_id).await?)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.records.get_tracks_for_release(release_id).await?)
    }

    /// A release's mediums, in order, which its tracks' places read against.
    pub async fn get_release_mediums(
        &self,
        release_id: &str,
    ) -> Result<bae_records::DbReleaseMediums, LibraryError> {
        Ok(self.records.get_release_mediums(release_id).await?)
    }

    /// The audio a catalog release is read against for the library release
    /// `release_id`: its tracks' stored lengths, and what its own files say
    /// it was copied from.
    #[cfg(desktop)]
    pub(super) async fn release_read_audio(
        &self,
        release_id: &str,
    ) -> Result<bae_catalogs::reading::ReadAudio, LibraryError> {
        use bae_import::extraction::release::{resolve_release_identity, ReleaseIdentityError};
        resolve_release_identity(&self.records, &self.diagnostics, release_id)
            .await
            .map_err(|error| match error {
                ReleaseIdentityError::Database(error) => LibraryError::Database(error),
                ReleaseIdentityError::NotFound { .. } => LibraryError::Refused(error.to_string()),
                ReleaseIdentityError::Panicked(failed) => LibraryError::from(failed),
            })?
            .read_audio()
            .map_err(LibraryError::from)
    }

    /// Test-only: seed a full album/release/tracks in one call. Production imports
    /// go through `finalize_import_atomic`.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        album_artists: &[bae_records::DbAlbumArtist],
        release: &DbRelease,
        mediums: &[bae_records::DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[bae_records::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.records
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
        mediums: &[bae_records::DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[bae_records::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.records.insert_release_with_tracks(release, mediums, tracks, track_artists).await?;
        Ok(())
    }

    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.records.get_releases_for_album(album_id).await?)
    }

    pub async fn check_releases_in_library(
        &self,
        checks: &[bae_records::LibraryCheck],
    ) -> Result<Vec<bae_records::LibraryStatus>, LibraryError> {
        Ok(self.records.check_releases_in_library(checks).await?)
    }

    pub fn subscribe_library_statuses(
        &self,
        initial: std::collections::BTreeSet<bae_records::LibraryCheck>,
    ) -> coven::ReconfigurableLiveQuery<
        std::collections::BTreeSet<bae_records::LibraryCheck>,
        Vec<bae_records::LibraryStatus>,
    > {
        self.records.subscribe_library_statuses(initial)
    }

    /// Every file of a release — audio files, and the metadata files (cover art,
    /// CUE sheets) that no track owns. Files belong to releases, not to albums or
    /// tracks; a track reaches its audio file through its `audio_segments`.
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.records.get_files_for_release(release_id).await?)
    }

    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id =
            self.records.find_album_id_for_release(release_id).await?.ok_or_else(|| {
                LibraryError::release_missing(release_id, super::ReleaseWork::Edit)
            })?;
        Ok(album_id)
    }

    /// Set which of an album's releases provides its cover art.
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.records.set_album_primary_release(album_id, primary_release_id).await?;

        Ok(())
    }

    /// Delete a release and its data. The rows go in one transaction, taking the
    /// album with them if this was its last release. Coven owns retention of
    /// files still referenced by committed uploads or shared history.
    pub async fn delete_release(&self, release_id: &str) -> Result<(), LibraryError> {
        self.records
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::release_missing(release_id, super::ReleaseWork::Edit))?;

        // The queue's rows leave in the delete's own transaction, and the
        // player, following the tracks it holds, moves off a deleted one.
        let deletion = self.records.plan_release_deletion(release_id).await?;
        // Deletion is a local transaction. Each coven file of the release goes
        // with its last reference, which takes its upload off the queue.
        self.records.delete_release_with_cleanup(deletion).await?;

        Ok(())
    }
}

/// The cover [`ImageRef`] for one release from its cover's version, or `None`
/// when it has no cover row. Free function so the manager's `cover_ref`,
/// the release-edit context, and one track's detail share one construction.
pub(crate) async fn cover_ref_for(
    records: &bae_records::Records,
    release_id: &str,
) -> Result<Option<ImageRef>, LibraryError> {
    Ok(bae_records::cover_ref(release_id, records.cover_version(release_id).await?))
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
