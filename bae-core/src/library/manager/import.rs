//! Import domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Test-only: seed a single file row. Production inserts files only as part of
    /// an import or edit transaction.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

    /// Insert all of an import's data in one transaction, so the release either
    /// exists complete or does not exist at all. Nothing of it is in the DB yet
    /// except the import record.
    ///
    /// Track rows come straight off `tracks_to_files` — each `TrackFile` owns the
    /// `DbTrack` (with its populated `duration_ms`) that gets inserted. There is no
    /// parallel list of tracks or durations.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finalize_import_atomic(
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
        artists: &[DbArtist],
        artist_external_id_updates: &[(String, DbArtist)],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        audio_segments: &[DbAudioSegment],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: &[(&DbLibraryImage, &[u8])],
        primary_release_id: Option<(&str, &str)>,
        identities: &[crate::import::ReleaseIdentity],
        local_path: &str,
        replacement_plans: &[ImportReplacementPlan],
    ) -> Result<(), LibraryError> {
        // The home's storage mode decides the blob layout (opaque hashed-by-id vs.
        // browsable readable paths); the manager owns config, so it reads the mode
        // here rather than threading it from the importer.
        let storage = self.config_handle.config().cloud_home.storage;
        let replacement_deletes: Vec<_> = replacement_plans
            .iter()
            .map(|plan| plan.db_delete.clone())
            .collect();
        let replacement_outcomes = self
            .database
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
                artists,
                artist_external_id_updates,
                files,
                audio_formats,
                audio_segments,
                library_image,
                artist_images,
                primary_release_id,
                identities,
                local_path,
                storage,
                &replacement_deletes,
            )
            .await?;
        if !replacement_plans.is_empty() {
            self.emit_outbox_changed().await;
        }
        for (index, plan) in replacement_plans.iter().enumerate() {
            let outcome = replacement_outcomes
                .get(index)
                .expect("finalize_import_atomic returns one outcome per replacement plan");
            self.evict_delete_blobs(plan.evict_blobs.clone()).await;

            if !plan.track_ids.is_empty() {
                self.emit(LibraryEvent::TracksDeleted {
                    track_ids: plan.track_ids.clone(),
                });
            }

            if outcome.album_deleted {
                self.emit_album_removed(&outcome.album_id, vec![outcome.release_id.clone()]);
            } else {
                self.emit_album_updated(&outcome.album_id).await;
                self.emit_release_removed(&outcome.album_id, &outcome.release_id)
                    .await;
            }
        }
        Ok(())
    }

    /// Persist one candidate's terminal identify verdict, keyed by its content
    /// hash. Device-local and never synced; every device derives its own.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_state(
        &self,
        state: &crate::db::NewImportCandidateState,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_import_candidate_state(state).await?)
    }

    /// Every stored candidate verdict, keyed by content hash. The queue is a few
    /// hundred rows at most, so the sweep reads it whole and decides in memory
    /// which candidates still need identifying.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self.database.load_import_candidate_states().await?)
    }

    /// Remove the release a failed import had already finalized, in one DB
    /// operation.
    pub async fn fail_import_and_delete_release(
        &self,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        // The DB layer deletes the release subtree and declares the cover/artist-
        // image blobs it orphans as deletions in the same coven write batch, so
        // coven records the durable local-cleanup intents that reclaim their
        // on-device bytes. Nothing to evict here.
        self.database
            .fail_import_and_delete_release(release_id)
            .await?;
        Ok(())
    }
}
