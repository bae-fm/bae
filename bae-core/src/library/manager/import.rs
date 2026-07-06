//! Import domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Add a file to the library
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

    /// Atomically insert all import data in a single transaction.
    /// Nothing is in the DB yet (except the import record and artists).
    /// The release either exists complete or doesn't exist at all.
    ///
    /// Track rows are read straight off `tracks_to_files` — each `TrackFile`
    /// owns the `DbTrack` (with its populated `duration_ms`) that gets
    /// inserted. There is no parallel list of tracks or durations.
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
        album_artists: &[crate::db::DbAlbumArtist],
        works: &[crate::db::DbWork],
        work_artists: &[crate::db::DbWorkArtist],
        work_parts: &[crate::db::DbWorkPart],
        track_works: &[crate::db::DbTrackWork],
        release_artist_roles: &[crate::db::DbReleaseArtistRole],
        track_artist_roles: &[crate::db::DbTrackArtistRole],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        audio_segments: &[DbAudioSegment],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        primary_release_id: Option<(&str, &str)>,
        import_id: &str,
        import_status: ImportOperationStatus,
        identities: &[crate::import::ReleaseIdentity],
        local_path: &str,
    ) -> Result<(), LibraryError> {
        // The home's storage mode decides the blob layout (opaque hashed-by-id vs.
        // browsable readable paths); the manager owns config, so it reads the mode
        // here rather than threading it from the importer.
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .finalize_import_atomic(
                album,
                release,
                tracks_to_files,
                metadata,
                track_artists,
                album_artists,
                works,
                work_artists,
                work_parts,
                track_works,
                release_artist_roles,
                track_artist_roles,
                files,
                audio_formats,
                audio_segments,
                library_image,
                primary_release_id,
                import_id,
                import_status,
                identities,
                local_path,
                storage,
            )
            .await?;
        Ok(())
    }

    pub async fn is_source_folder_name_imported(&self, name: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_source_folder_name_imported(name).await?)
    }

    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), LibraryError> {
        Ok(self.database.insert_import(import).await?)
    }

    /// Update the status of an import operation
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), LibraryError> {
        Ok(self.database.update_import_status(id, status).await?)
    }

    /// Record an error for an import operation
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), LibraryError> {
        Ok(self.database.update_import_error(id, error).await?)
    }

    /// Mark an import failed and remove its finalized release in one DB
    /// operation.
    pub async fn fail_import_and_delete_release(
        &self,
        import_id: &str,
        release_id: &str,
        error: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .fail_import_and_delete_release(import_id, release_id, error)
            .await?)
    }

    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, LibraryError> {
        Ok(self.database.get_active_imports().await?)
    }

    /// Delete an import record (used by UI to dismiss stuck imports)
    pub async fn delete_import(&self, id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_import(id).await?)
    }
}
