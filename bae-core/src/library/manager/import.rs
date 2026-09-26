//! Import domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub(crate) async fn set_grouping_skipped(
        &self,
        key: &str,
        skipped: bool,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.set_grouping_skipped(key, skipped).await?)
    }

    /// The release stored at `key`, when it may be worked on — or why a
    /// grouping's release cannot be.
    pub(crate) async fn load_release_candidate(
        &self,
        key: &str,
    ) -> Result<
        Result<
            Option<crate::import::folder_scanner::FolderCandidate>,
            crate::import::GroupingBlock,
        >,
        LibraryError,
    > {
        Ok(self.database.load_release_candidate(key).await?)
    }

    /// Read `members`, each as the caller read it, as one release under the
    /// new grouping `key`.
    pub(crate) async fn combine_releases(
        &self,
        key: String,
        members: Vec<crate::import::FolderCandidate>,
    ) -> Result<Result<crate::db::GroupingChanges, crate::import::GroupingBlock>, LibraryError>
    {
        Ok(self.database.combine_releases(key, members).await?)
    }

    /// Undo the grouping of releases picked together at `key`, returning
    /// them as they are stored, and the releases of groupings rebuilt because
    /// the files of the folder it read are free again.
    pub(crate) async fn separate_picked_grouping(
        &self,
        key: &str,
    ) -> Result<
        (
            Vec<crate::import::folder_scanner::ScanItem>,
            crate::db::GroupingChanges,
        ),
        LibraryError,
    > {
        Ok(self.database.separate_picked_grouping(key).await?)
    }

    /// How the grouping `key` reads, or `None` when no grouping has it.
    pub(crate) async fn load_grouping(
        &self,
        key: &str,
    ) -> Result<Option<crate::db::GroupingFacts>, LibraryError> {
        Ok(self.database.load_grouping(key).await?)
    }

    pub(crate) fn subscribe_import_list(
        &self,
        initial: crate::import::ImportListRequest,
    ) -> coven::ReconfigurableLiveQuery<
        crate::import::ImportListRequest,
        crate::import::ImportListProjection,
    > {
        self.database.subscribe_import_list(initial)
    }

    pub(crate) fn subscribe_folder_scan_progress(
        &self,
    ) -> coven::LiveQuery<crate::import::FolderScanProgress> {
        self.database.subscribe_folder_scan_progress()
    }

    pub(crate) async fn load_import_list(
        &self,
        request: crate::import::ImportListRequest,
    ) -> Result<crate::import::ImportListProjection, LibraryError> {
        Ok(self.database.load_import_list(request).await?)
    }

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

    pub(crate) async fn load_chosen_folder(
        &self,
        root: String,
        chosen: std::path::PathBuf,
    ) -> Result<crate::import::list::ChosenFolderRead, LibraryError> {
        Ok(self.database.load_chosen_folder(root, chosen).await?)
    }

    pub(crate) async fn first_import_candidate_among(
        &self,
        request: crate::import::ImportListRequest,
        keys: std::collections::HashSet<String>,
    ) -> Result<Option<String>, LibraryError> {
        Ok(self
            .database
            .first_import_candidate_among(request, keys)
            .await?)
    }

    pub(crate) fn subscribe_import_candidate(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<String>,
        Option<crate::import::ImportCandidateDetailProjection>,
    > {
        self.database.subscribe_import_candidate(initial)
    }

    /// Record the pane's per-candidate state between visits.
    pub(crate) async fn save_import_candidate_session(
        &self,
        content_hash: &str,
        session: &crate::import::CandidateSession,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .save_import_candidate_session(content_hash, session)
            .await?)
    }

    /// Open the pane on Find online for every one of these candidates.
    pub(crate) async fn open_import_candidate_sessions_on_find_online(
        &self,
        content_hashes: Vec<String>,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .open_import_candidate_sessions_on_find_online(content_hashes)
            .await?)
    }

    /// Record what a candidate's identification asks about.
    pub(crate) async fn save_import_candidate_lookup_choices(
        &self,
        content_hash: &str,
        choices: &crate::import::LookupChoices,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .save_import_candidate_lookup_choices(content_hash, choices)
            .await?)
    }

    pub(crate) async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::ImportCandidateDetailProjection>, LibraryError> {
        Ok(self.database.load_import_candidate(key).await?)
    }

    /// Every candidate the queue sweep is responsible for, with its files.
    pub(crate) async fn load_sweepable_candidates(
        &self,
    ) -> Result<Vec<crate::import::FolderCandidate>, LibraryError> {
        Ok(self.database.load_sweepable_candidates().await?)
    }

    pub async fn start_import_service(
        &self,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<crate::import::ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            runtime_handle,
            self.clone(),
            self.preparations.clone(),
            self.clock.clone(),
            self.ids.clone(),
        )
        .await
    }

    /// [`Self::start_import_service`] reading folders' tags through
    /// `file_tags`.
    #[cfg(test)]
    pub(crate) fn start_import_service_reading_tags_with(
        &self,
        runtime_handle: tokio::runtime::Handle,
        file_tags: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> crate::import::ImportServiceHandle {
        crate::import::ImportService::start_reading_tags_with(
            runtime_handle,
            self.clone(),
            self.preparations.clone(),
            self.clock.clone(),
            self.ids.clone(),
            file_tags,
        )
    }

    /// The fetched release `release` names, as its extraction stored it, or
    /// `None` when nothing has fetched it.
    pub(crate) async fn load_source_release(
        &self,
        release: &crate::import::MetadataRef,
    ) -> Result<Option<crate::import::source_release::SourceRelease>, LibraryError> {
        Ok(self.database.load_source_release(release).await?)
    }

    pub(crate) async fn save_source_release(
        &self,
        release: &crate::import::source_release::SourceRelease,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_source_release(release).await?)
    }

    pub async fn load_watched_import_folders(
        &self,
    ) -> Result<Vec<crate::import::WatchedFolder>, LibraryError> {
        Ok(self.database.load_watched_import_folders().await?)
    }

    pub(crate) async fn is_release_candidate_skipped(
        &self,
        candidate: &crate::import::folder_scanner::FolderCandidate,
    ) -> Result<bool, LibraryError> {
        Ok(self
            .database
            .is_release_candidate_skipped(candidate)
            .await?)
    }

    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &str,
    ) -> Result<std::collections::HashSet<String>, LibraryError> {
        Ok(self
            .database
            .load_skipped_import_candidates(watched_folder_path)
            .await?)
    }

    pub async fn add_watched_import_folder(&self, path: &str) -> Result<bool, LibraryError> {
        Ok(self.database.add_watched_import_folder(path).await?)
    }

    pub async fn remove_watched_import_folder(
        &self,
        path: &str,
    ) -> Result<Option<Vec<String>>, LibraryError> {
        Ok(self.database.remove_watched_import_folder(path).await?)
    }

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

    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume it is on as the scan finds it now.
    pub async fn begin_folder_scan(&self, watched_folder_path: &str) -> Result<u64, LibraryError> {
        let volume =
            crate::import::volume::volume_kind(std::path::Path::new(watched_folder_path)).await;
        self.begin_folder_scan_on(watched_folder_path, volume).await
    }

    /// [`Self::begin_folder_scan`] for a scan that has already asked which
    /// volume the folder is on.
    pub(crate) async fn begin_folder_scan_on(
        &self,
        watched_folder_path: &str,
        volume: crate::import::VolumeKind,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .begin_folder_scan(watched_folder_path, volume)
            .await?)
    }

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

    pub async fn load_folder_scan_directories(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<(String, i64)>, LibraryError> {
        Ok(self
            .database
            .load_folder_scan_directories(watched_folder_path)
            .await?)
    }

    /// Store one scan item under `generation`, seeded with `file_metadata` —
    /// the reading [`Self::scan_item_seed`] took of it, which the caller takes
    /// before the folder-state commit lock and this stores under it.
    pub(crate) async fn save_folder_scan_item_with_seed(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &crate::import::folder_scanner::ScanItem,
        file_metadata: Option<crate::import::file_metadata_seed::FileMetadataSeed>,
        folder_date: Option<crate::import::folder_scanner::FolderDate>,
    ) -> Result<Option<crate::db::ScanItemWrite>, LibraryError> {
        Ok(self
            .database
            .save_folder_scan_item_with_seed(
                watched_folder_path,
                generation,
                item,
                file_metadata,
                folder_date,
            )
            .await?)
    }

    /// What a candidate a pass is about to store under `generation` starts
    /// from: its own file tags when the pre-fill is on, nothing otherwise.
    ///
    /// Reads the folder's audio files, which on a network share takes as long
    /// as the share does, so it runs on a blocking thread and never under the
    /// folder-state commit lock every pane control waits on.
    pub(crate) async fn scan_item_seed(
        &self,
        item: &crate::import::folder_scanner::ScanItem,
        generation: u64,
        reader: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<Option<crate::import::file_metadata_seed::FileMetadataSeed>, LibraryError> {
        if !self.config_handle.config().prefs.prefill_with_file_metadata {
            return Ok(None);
        }
        self.file_metadata_seed(item, generation, reader).await
    }

    /// The folder read as its own files describe it, for a candidate this scan
    /// is about to store.
    ///
    /// A folder whose tags cannot be read gets the blank draft instead: the
    /// pre-fill is a default, not a command, so a folder nobody can read tags
    /// from still joins the queue and says so in the log.
    async fn file_metadata_seed(
        &self,
        item: &crate::import::folder_scanner::ScanItem,
        generation: u64,
        reader: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<Option<crate::import::file_metadata_seed::FileMetadataSeed>, LibraryError> {
        use crate::import::folder_scanner::ScanItem;
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            return Ok(None);
        };
        // A candidate that already holds a draft is not re-seeded, so its tags
        // are not read either: a rescan of a folder nobody touched reads no
        // audio file at all.
        if self
            .database
            .candidate_has_draft(&candidate.files.content_hash())
            .await?
        {
            return Ok(None);
        }
        let folder = candidate.path.display().to_string();
        let candidate = candidate.clone();
        let clock = self.clock.clone();
        let ids = self.ids.clone();
        let read = tokio::task::spawn_blocking(move || {
            crate::import::file_metadata_seed::FileMetadataSeed::read(
                &candidate,
                generation,
                reader.as_ref(),
                clock.as_ref(),
                ids.as_ref(),
            )
        })
        .await
        .map_err(|error| LibraryError::Import(format!("file tag read task failed: {error}")))?;
        match read {
            Ok(seed) => Ok(Some(seed)),
            Err(error) => {
                tracing::warn!(
                    "{folder} starts on a blank draft: its file tags could not be read: {error}"
                );
                Ok(None)
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &crate::import::folder_scanner::ScanItem,
    ) -> Result<Option<crate::db::ScanItemWrite>, LibraryError> {
        let file_metadata = self
            .scan_item_seed(
                item,
                generation,
                std::sync::Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
            )
            .await?;
        self.save_folder_scan_item_with_seed(
            watched_folder_path,
            generation,
            item,
            file_metadata,
            None,
        )
        .await
    }

    pub async fn finish_folder_scan(
        &self,
        watched_folder_path: &str,
        generation: u64,
        error: Option<&str>,
    ) -> Result<Option<crate::db::FinishedScan>, LibraryError> {
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

    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &str,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self
            .database
            .load_folder_scan_items(watched_folder_path)
            .await?)
    }

    pub async fn load_all_folder_scan_items(
        &self,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_all_folder_scan_items().await?)
    }

    pub async fn load_folder_scan_item(
        &self,
        entry_key: &str,
    ) -> Result<Option<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_folder_scan_item(entry_key).await?)
    }

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
    /// except the import record. A Remote import (`remote`) records its
    /// make-Remote in the same write and gets back the outbox revision that
    /// shows its uploads queued.
    ///
    /// Track rows come straight off `tracks_to_files` — each `TrackFile` owns the
    /// `DbTrack` (with its populated `duration_ms`) that gets inserted. There is no
    /// parallel list of tracks or durations.
    ///
    /// The release's artist credits are resolved inside the write. The Discogs
    /// pictures of the artists it creates are staged before the write, so they
    /// are chosen from a read of the library just before it; the write refuses
    /// to commit if its own resolution creates different artists, and the
    /// import fails with that rather than landing a picture without its artist.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finalize_import_atomic(
        &self,
        guard: crate::db::ImportCommitGuard,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        rows: crate::db::ImportRows<'_>,
        files: Vec<crate::import::service::PreparedImportFile>,
        library_image: Option<(&DbLibraryImage, &[u8])>,
        prepared_artist_images: &[crate::import::PreparedArtistImage],
        primary_release_id: Option<(&str, &str)>,
        replacement_plans: &[ImportReplacementPlan],
        remote: Option<crate::db::RemoteImport>,
    ) -> Result<Option<u64>, LibraryError> {
        let expected = self
            .database
            .resolve_artists(rows.artists.credits, rows.artists.picked)
            .await?;
        let expected_new_artists: Vec<String> = expected
            .inserts
            .iter()
            .map(|artist| artist.id.clone())
            .collect();
        let artist_images =
            self.materialize_prepared_artist_images(&expected.inserts, prepared_artist_images)?;
        let artist_images: Vec<_> = artist_images
            .iter()
            .map(|(image, bytes)| (image, bytes.as_slice()))
            .collect();
        // The home's storage mode decides the blob layout (opaque hashed-by-id vs.
        // browsable readable paths); the manager owns config, so it reads the mode
        // here rather than threading it from the importer.
        let storage = self.config_handle.config().cloud_home.storage;
        let replacement_deletes: Vec<_> = replacement_plans
            .iter()
            .map(|plan| plan.deletion.clone())
            .collect();
        self.database
            .finalize_import_atomic(
                guard,
                album,
                release,
                tracks_to_files,
                rows,
                files,
                library_image,
                crate::db::NewArtistImages {
                    expected_new_artists: &expected_new_artists,
                    images: &artist_images,
                },
                primary_release_id,
                storage,
                &replacement_deletes,
                remote,
            )
            .await?;
        // The outbox value that already shows the queued uploads (or the
        // replaced releases' unwinding), whose revision a Remote import hands
        // back as its receipt.
        let outbox_revision = if remote.is_some() || !replacement_plans.is_empty() {
            Some(self.emit_outbox_changed().await)
        } else {
            None
        };
        for plan in replacement_plans {
            if !plan.track_ids.is_empty() {
                self.emit(LibraryEvent::TracksDeleted {
                    track_ids: plan.track_ids.clone(),
                });
            }
        }
        Ok(outbox_revision.filter(|_| remote.is_some()))
    }

    /// Every stored candidate row, keyed by content hash. The queue is a few
    /// hundred rows at most, so the sweep reads it whole and decides in memory
    /// which candidates still need identifying.
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self.database.load_import_candidate_states().await?)
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_state(content_hash)
            .await?)
    }

    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<crate::db::DbCandidatePaneRows, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_pane_rows(content_hash)
            .await?)
    }

    pub async fn load_import_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbCandidateImportPreparation>, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_preparation(content_hash)
            .await?)
    }

    pub async fn load_import_candidate_prepared_assets(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::CandidatePreparedAssets, LibraryError> {
        Ok(self
            .database
            .load_import_candidate_prepared_assets(content_hash)
            .await?)
    }

    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &crate::import::ImportFailure,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .save_import_candidate_failure(content_hash, edit_revision, failure)
            .await?)
    }

    pub async fn clear_import_candidate_failure(
        &self,
        content_hash: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .clear_import_candidate_failure(content_hash)
            .await?)
    }

    /// Every candidate's user-set file decisions, keyed by content hash — what
    /// a folder scan needs so the roles it reports are the ones the user
    /// settled, not only the ones its filenames propose.
    pub async fn load_stored_candidate_edits(
        &self,
    ) -> Result<crate::import::folder_scanner::StoredCandidateEdits, LibraryError> {
        Ok(self.database.load_stored_candidate_edits().await?)
    }

    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::folder_scanner::CandidateFileEdits, LibraryError> {
        Ok(self
            .database
            .load_candidate_file_edits(content_hash)
            .await?)
    }

    /// Store the reading a scan settled on for one folder. Never disturbs the
    /// scan that produced it, and never replaces the user's own answer.
    pub async fn record_scanned_folder_release_decision(
        &self,
        key: &crate::import::folder_scanner::FolderReleaseDecisionKey,
        decision: crate::import::folder_scanner::FolderReleaseDecision,
        grouping: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .record_scanned_folder_release_decision(key, decision, grouping)
            .await?)
    }

    /// The generation `watched_folder_path` stands at, or `None` for a root
    /// that has never been read.
    pub(crate) async fn current_folder_scan_generation(
        &self,
        watched_folder_path: &str,
    ) -> Result<Option<u64>, LibraryError> {
        Ok(self
            .database
            .current_folder_scan_generation(watched_folder_path)
            .await?)
    }

    /// Begin reading one folder of a watched root again.
    pub(crate) async fn begin_folder_reading(
        &self,
        watched_folder_path: &str,
    ) -> Result<crate::db::FolderReadingStamp, LibraryError> {
        Ok(self
            .database
            .begin_folder_reading(watched_folder_path)
            .await?)
    }

    /// Store one folder's new reading, with the decision that changed it, as
    /// one write.
    pub(crate) async fn commit_folder_reading(
        &self,
        commit: crate::db::FolderReadingCommit,
    ) -> Result<crate::db::FolderReadingWrite, LibraryError> {
        Ok(self.database.commit_folder_reading(commit).await?)
    }

    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &str,
    ) -> Result<crate::import::folder_scanner::FolderReleaseDecisions, LibraryError> {
        Ok(self
            .database
            .load_folder_release_decisions(watched_folder_path)
            .await?)
    }
}
