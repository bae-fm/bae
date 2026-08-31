//! Import domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn subscribe_import_list(
        &self,
        initial: crate::import::ImportListRequest,
    ) -> coven::ReconfigurableLiveQuery<
        crate::import::ImportListRequest,
        crate::import::ImportListProjection,
    > {
        self.database.subscribe_import_list(initial)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn load_import_list(
        &self,
        request: crate::import::ImportListRequest,
    ) -> Result<crate::import::ImportListProjection, LibraryError> {
        Ok(self.database.load_import_list(request).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn locate_import_candidate(
        &self,
        request: crate::import::ImportListRequest,
        candidate_key: &str,
    ) -> Result<Option<crate::import::ImportCandidateListLocation>, LibraryError> {
        Ok(self
            .database
            .locate_import_candidate(request, candidate_key)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn subscribe_import_candidate(
        &self,
        key: &str,
    ) -> coven::LiveQuery<Option<crate::import::ImportCandidateDetailProjection>> {
        self.database.subscribe_import_candidate(key)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::ImportCandidateDetailProjection>, LibraryError> {
        Ok(self.database.load_import_candidate(key).await?)
    }

    /// Every candidate the queue sweep is responsible for, with its files.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn load_sweepable_candidates(
        &self,
    ) -> Result<Vec<crate::import::FolderCandidate>, LibraryError> {
        Ok(self.database.load_sweepable_candidates().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn start_import_service(
        &self,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<crate::import::ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            runtime_handle,
            self.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn source_release_payload_for_test(
        &self,
        source: crate::import::PayloadSource,
        release_id: &str,
    ) -> Result<Option<String>, LibraryError> {
        Ok(self
            .database
            .load_source_release_payloads(&[(source, release_id.to_string())])
            .await?
            .remove(&(source, release_id.to_string())))
    }

    #[cfg(test)]
    pub(crate) async fn save_source_release_payloads_for_test(
        &self,
        rows: &[crate::db::DbSourceReleasePayload],
    ) -> Result<(), LibraryError> {
        self.database.save_source_release_payloads(rows).await?;
        Ok(())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn load_release_payloads(
        &self,
        release: &crate::import::MetadataRef,
    ) -> Result<Option<crate::import::payloads::ReleasePayloads>, crate::import::ImportError> {
        crate::import::payloads::load(&self.database, release).await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn store_release_payloads(
        &self,
        payloads: &crate::import::payloads::ReleasePayloads,
    ) -> Result<(), LibraryError> {
        crate::import::payloads::store(&self.database, payloads, self.clock.now()).await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_import_folder_registry(
        &self,
    ) -> Result<crate::import::ImportFolderRegistry, LibraryError> {
        Ok(self.database.load_import_folder_registry().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn add_watched_import_folder(&self, path: &str) -> Result<bool, LibraryError> {
        Ok(self.database.add_watched_import_folder(path).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn remove_watched_import_folder(
        &self,
        path: &str,
    ) -> Result<Option<Vec<String>>, LibraryError> {
        Ok(self.database.remove_watched_import_folder(path).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn set_import_candidate_skipped(
        &self,
        watched_folder_path: &str,
        relative_candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, LibraryError> {
        Ok(self
            .database
            .set_import_candidate_skipped(watched_folder_path, relative_candidate_path, skipped)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn begin_folder_scan(&self, watched_folder_path: &str) -> Result<u64, LibraryError> {
        Ok(self.database.begin_folder_scan(watched_folder_path).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn record_folder_scan_directories(
        &self,
        watched_folder_path: &str,
        directories: &[(String, i64)],
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .record_folder_scan_directories(watched_folder_path, directories)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_folder_scan_directories(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<(String, i64)>, LibraryError> {
        Ok(self
            .database
            .load_folder_scan_directories(watched_folder_path)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &crate::import::folder_scanner::ScanItem,
    ) -> Result<Option<crate::db::ScanItemWrite>, LibraryError> {
        let initial_metadata_source = self.config_handle.config().default_import_metadata_source;
        Ok(self
            .database
            .save_folder_scan_item_with_initial_source(
                watched_folder_path,
                generation,
                item,
                initial_metadata_source,
            )
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn finish_folder_scan(
        &self,
        watched_folder_path: &str,
        generation: u64,
        error: Option<&str>,
    ) -> Result<Option<Vec<String>>, LibraryError> {
        Ok(self
            .database
            .finish_folder_scan(watched_folder_path, generation, error)
            .await?)
    }

    #[cfg(test)]
    pub async fn load_folder_scan_snapshots(
        &self,
    ) -> Result<Vec<crate::db::DbFolderScanSnapshot>, LibraryError> {
        Ok(self.database.load_folder_scan_snapshots().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self
            .database
            .load_folder_scan_items(watched_folder_path)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_all_folder_scan_items(
        &self,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_all_folder_scan_items().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_folder_scan_item(
        &self,
        entry_key: &str,
    ) -> Result<Option<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_folder_scan_item(entry_key).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
    ) -> Result<Option<crate::db::DbCandidateFileTagSnapshot>, LibraryError> {
        Ok(self
            .database
            .load_candidate_file_tag_snapshot(watched_folder_path, candidate_path)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn replace_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    ) -> Result<bool, LibraryError> {
        Ok(self
            .database
            .replace_candidate_file_tag_snapshot(watched_folder_path, candidate_path, snapshot)
            .await?)
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
        files: Vec<crate::import::service::PreparedImportFile>,
        audio_formats: &[DbAudioFormat],
        audio_segments: &[DbAudioSegment],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: &[(&DbLibraryImage, &[u8])],
        primary_release_id: Option<(&str, &str)>,
        identities: &[crate::import::ReleaseIdentity],
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
        self.database
            .finalize_import_atomic(
                album,
                release,
                tracks_to_files,
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
                storage,
                &replacement_deletes,
            )
            .await?;
        if !replacement_plans.is_empty() {
            self.emit_outbox_changed().await;
        }
        for plan in replacement_plans {
            self.evict_delete_blobs(plan.evict_blobs.clone()).await;

            if !plan.track_ids.is_empty() {
                self.emit(LibraryEvent::TracksDeleted {
                    track_ids: plan.track_ids.clone(),
                });
            }
        }
        Ok(())
    }

    /// Persist one candidate's terminal identify verdict, keyed by its content
    /// hash. Device-local and never synced; every device derives its own.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_verdict(
        &self,
        verdict: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.save_import_candidate_verdict(verdict).await?)
    }

    /// Persist one candidate's user-set file decisions and clear whatever
    /// identification had concluded about it — see
    /// [`crate::db::Database::save_import_candidate_file_edits`] for why those
    /// are one operation.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_file_edits(
        &self,
        content_hash: &str,
        folder_path: &str,
        expected_revision: u64,
        edits: &crate::import::folder_scanner::CandidateFileEdits,
        settled_candidates: &[(String, crate::import::folder_scanner::CategorizedFiles)],
    ) -> Result<(u64, Vec<crate::import::folder_scanner::FolderCandidate>), LibraryError> {
        Ok(self
            .database
            .save_import_candidate_file_edits(
                content_hash,
                folder_path,
                expected_revision,
                edits,
                settled_candidates,
            )
            .await?)
    }

    /// Every stored candidate row, keyed by content hash. The queue is a few
    /// hundred rows at most, so the sweep reads it whole and decides in memory
    /// which candidates still need identifying.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self.database.load_import_candidate_states().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_state(content_hash)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn replace_candidate_metadata(
        &self,
        content_hash: &str,
        folder_path: &str,
        draft: &crate::import::RawReleaseEdit,
        provenance: Option<&crate::import::MetadataProvenance>,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .replace_candidate_metadata(content_hash, folder_path, draft, provenance)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn replace_candidate_file_tags_metadata(
        &self,
        watched_folder_path: &str,
        candidate_path: &str,
        content_hash: &str,
        snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
        draft: &crate::import::RawReleaseEdit,
        cover: Option<&crate::import::CoverSelection>,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .replace_candidate_file_tags_metadata(
                watched_folder_path,
                candidate_path,
                content_hash,
                snapshot,
                draft,
                cover,
            )
            .await?)
    }

    /// Everything a person settled about one candidate through its pane.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<crate::db::DbCandidatePaneRows, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_pane_rows(content_hash)
            .await?)
    }

    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        folder_path: &str,
        edit_revision: u64,
        failure: &crate::import::ImportFailure,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .save_import_candidate_failure(content_hash, folder_path, edit_revision, failure)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn clear_import_candidate_failure(
        &self,
        content_hash: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .clear_import_candidate_failure(content_hash)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_cover(
        &self,
        content_hash: &str,
        cover: &crate::import::CoverSelection,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .save_import_candidate_cover(content_hash, cover)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_edit_field(
        &self,
        content_hash: &str,
        field: crate::import::CandidateEditField,
        value: &str,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .save_import_candidate_edit_field(content_hash, field, value)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn replace_import_candidate_album_artists(
        &self,
        content_hash: &str,
        assignments: &[crate::import::ArtistAssignment],
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .replace_import_candidate_album_artists(content_hash, assignments)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn save_import_candidate_track_edit(
        &self,
        content_hash: &str,
        edit: &crate::import::CandidateTrackEdit,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .save_import_candidate_track_edit(content_hash, edit)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn replace_import_candidate_track_artists(
        &self,
        content_hash: &str,
        track_ids: &[String],
        assignments: &crate::import::TrackArtistAssignments,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .replace_import_candidate_track_artists(content_hash, track_ids, assignments)
            .await?)
    }

    /// Every candidate's user-set file decisions, keyed by content hash — what
    /// a folder scan needs so the roles it reports are the ones the user
    /// settled, not only the ones its filenames propose.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_stored_candidate_edits(
        &self,
    ) -> Result<crate::import::folder_scanner::StoredCandidateEdits, LibraryError> {
        Ok(self.database.load_stored_candidate_edits().await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::folder_scanner::CandidateFileEdits, LibraryError> {
        Ok(self
            .database
            .load_candidate_file_edits(content_hash)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn set_folder_release_decision(
        &self,
        key: &crate::import::folder_scanner::FolderReleaseDecisionKey,
        decision: crate::import::folder_scanner::FolderReleaseDecision,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .set_folder_release_decision(
                key,
                decision,
                crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,
            )
            .await?)
    }

    /// Store the reading a scan settled on for one folder. Never disturbs the
    /// scan that produced it, and never replaces the user's own answer.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn record_scanned_folder_release_decision(
        &self,
        key: &crate::import::folder_scanner::FolderReleaseDecisionKey,
        decision: crate::import::folder_scanner::FolderReleaseDecision,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .record_scanned_folder_release_decision(key, decision)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn set_folder_release_decisions(
        &self,
        decisions: &[(
            crate::import::folder_scanner::FolderReleaseDecisionKey,
            crate::import::folder_scanner::FolderReleaseDecision,
        )],
    ) -> Result<(u64, Vec<String>), LibraryError> {
        Ok(self
            .database
            .set_folder_release_decisions(
                decisions,
                crate::import::folder_scanner::FolderReleaseDecisionAuthor::User,
            )
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &str,
    ) -> Result<crate::import::folder_scanner::FolderReleaseDecisions, LibraryError> {
        Ok(self
            .database
            .load_folder_release_decisions(watched_folder_path)
            .await?)
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
