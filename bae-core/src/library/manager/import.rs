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

    /// Undo the grouping of releases picked together at `key`, returning them
    /// as stored and the releases of groupings rebuilt now that the files of
    /// the folder it read are free.
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

    pub(crate) async fn load_folder_scan_progress(
        &self,
    ) -> Result<crate::import::FolderScanProgress, LibraryError> {
        Ok(self.database.load_folder_scan_progress().await?)
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

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no list open: a list opened for the read and closed after it.
    pub(crate) async fn load_import_list_once(
        &self,
        view: crate::import::ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<crate::import::ImportListProjection, LibraryError> {
        let list = self.open_import_list(view).await?;
        let projection = self
            .load_import_list(crate::import::ImportListRequest { list, windows })
            .await;
        self.close_import_list(list).await?;
        projection
    }

    pub(crate) async fn locate_import_candidate(
        &self,
        list: crate::import::ImportListId,
        candidate_key: &str,
    ) -> Result<Option<crate::import::ImportCandidateListLocation>, LibraryError> {
        Ok(self
            .database
            .locate_import_candidate(list, candidate_key)
            .await?)
    }

    pub(crate) async fn first_identifying_import_candidate(
        &self,
        list: crate::import::ImportListId,
    ) -> Result<Option<String>, LibraryError> {
        Ok(self
            .database
            .first_identifying_import_candidate(list)
            .await?)
    }

    pub(crate) async fn open_import_list(
        &self,
        view: crate::import::ImportListView,
    ) -> Result<crate::import::ImportListId, LibraryError> {
        Ok(self.database.open_import_list(view).await?)
    }

    pub(crate) async fn close_import_list(
        &self,
        list: crate::import::ImportListId,
    ) -> Result<(), LibraryError> {
        Ok(self.database.close_import_list(list).await?)
    }

    pub(crate) async fn close_import_lists(&self) -> Result<(), LibraryError> {
        Ok(self.database.close_import_lists().await?)
    }

    pub(crate) async fn set_import_list_view(
        &self,
        list: crate::import::ImportListId,
        view: crate::import::ImportListView,
    ) -> Result<u64, LibraryError> {
        Ok(self.database.set_import_list_view(list, view).await?)
    }

    pub(crate) async fn set_import_list_group_folded(
        &self,
        list: crate::import::ImportListId,
        group: crate::import::FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .set_import_list_group_folded(list, group, folded)
            .await?)
    }

    pub(crate) async fn set_import_list_groups_folded(
        &self,
        list: crate::import::ImportListId,
        folded: bool,
    ) -> Result<u64, LibraryError> {
        Ok(self
            .database
            .set_import_list_groups_folded(list, folded)
            .await?)
    }

    pub(crate) async fn change_import_list_selection(
        &self,
        list: crate::import::ImportListId,
        change: crate::import::selection::SelectionChange,
    ) -> Result<crate::db::SelectionWrite, LibraryError> {
        Ok(self
            .database
            .change_import_list_selection(list, change)
            .await?)
    }

    pub(crate) async fn select_all_in_import_list(
        &self,
        list: crate::import::ImportListId,
    ) -> Result<u64, LibraryError> {
        Ok(self.database.select_all_in_import_list(list).await?)
    }

    pub(crate) async fn load_import_list_selected(
        &self,
        list: crate::import::ImportListId,
    ) -> Result<crate::import::selection::SelectedRows, LibraryError> {
        Ok(self.database.load_import_list_selected(list).await?)
    }

    pub(crate) fn subscribe_invalid_candidate(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<crate::import::InvalidCandidateDetail>>
    {
        self.database.subscribe_invalid_candidate(initial)
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

    /// Move the pane of every one of these candidates by `pane_move`.
    pub(crate) async fn move_import_candidate_panes(
        &self,
        content_hashes: Vec<String>,
        pane_move: crate::import::PaneMove,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .move_import_candidate_panes(content_hashes, pane_move)
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

    pub async fn start_import_service(
        &self,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<crate::import::ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            runtime_handle,
            self.clone(),
            self.database.clone(),
            self.diagnostics.clone(),
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
            self.database.clone(),
            self.diagnostics.clone(),
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

    /// The watched folder that holds `path` inside it, not `path` itself.
    pub(crate) async fn watched_import_folder_above(
        &self,
        path: &str,
    ) -> Result<Option<String>, LibraryError> {
        Ok(self.database.watched_import_folder_above(path).await?)
    }

    /// The watched folders inside `path`, not `path` itself, in their order.
    pub(crate) async fn watched_import_folders_below(
        &self,
        path: &str,
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.watched_import_folders_below(path).await?)
    }

    /// Whether `path`, spelled as the store keys it, is a watched folder.
    pub(crate) async fn is_watched_import_folder(&self, path: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_watched_import_folder(path).await?)
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

    #[cfg(any(test, feature = "test-utils"))]
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

    /// Stop watching `roots`, watching `parent` in their place when given.
    /// See [`crate::db::Database::remove_watched_import_folders`].
    pub async fn remove_watched_import_folders(
        &self,
        roots: Vec<String>,
        parent: Option<String>,
    ) -> Result<Option<Vec<String>>, LibraryError> {
        Ok(self
            .database
            .remove_watched_import_folders(roots, parent)
            .await?)
    }

    pub async fn set_import_candidate_skipped(
        &self,
        candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, LibraryError> {
        Ok(self
            .database
            .set_import_candidate_skipped(candidate_path, skipped)
            .await?)
    }

    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume it is on.
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

    /// Store scan items under `generation` in one write, each with the seed
    /// the caller took from [`Self::scan_item_seed`] before the folder-state
    /// commit lock.
    pub(crate) async fn save_folder_scan_items(
        &self,
        watched_folder_path: &str,
        generation: u64,
        items: Vec<crate::db::ScanItemToWrite>,
    ) -> Result<
        Option<
            Vec<(
                crate::import::folder_scanner::ScanItem,
                crate::db::ScanItemWrite,
            )>,
        >,
        LibraryError,
    > {
        Ok(self
            .database
            .save_folder_scan_items(watched_folder_path, generation, items)
            .await?)
    }

    /// The folder about to be stored as its own file tags describe it: the
    /// reading of its tracks' tags its scan row keeps, and the draft and cover
    /// they seed a candidate that has none with. It is taken from what `reads`
    /// — the pass that found it — and the reading stored for it hold, and read
    /// where neither knows. A folder whose tags cannot be read keeps no
    /// reading and, when it has no draft, starts on a blank one; it is logged
    /// rather than kept out of the queue. Reading audio files can be slow on a
    /// network share, so it runs on a blocking thread and outside the
    /// folder-state commit lock every pane control waits on.
    ///
    /// A tentative folder whose files a settled entry already reads takes no
    /// seed and opens no file: the store keeps that entry and writes nothing
    /// for the folder.
    pub(crate) async fn scan_item_seed(
        &self,
        item: &crate::import::folder_scanner::ScanItem,
        generation: u64,
        reads: std::sync::Arc<crate::import::folder_scanner::FileReads>,
    ) -> Result<crate::db::ScanItemSeed, LibraryError> {
        use crate::db::ScanItemSeed;
        use crate::import::folder_scanner::ScanItem;
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            return Ok(ScanItemSeed::Tags(None));
        };
        if let (ScanItem::Discovered(_), Some(coverage)) = (item, item.coverage()) {
            if self
                .database
                .settled_entry_reads(&candidate.watched_folder_path, &coverage)
                .await?
            {
                return Ok(ScanItemSeed::AlreadySettled);
            }
        }
        let folder = candidate.path.display().to_string();
        let stored = self
            .database
            .load_stored_file_tag_reading(
                &candidate.watched_folder_path,
                &candidate.key(),
                &candidate.file_root,
            )
            .await?;
        let candidate = candidate.clone();
        let read = tokio::task::spawn_blocking(move || {
            crate::import::file_metadata_seed::FileMetadataSeed::read(
                &candidate,
                generation,
                &reads,
                stored.as_ref(),
            )
        })
        .await
        .map_err(|error| LibraryError::Internal(format!("file tag read task failed: {error}")))?;
        match read {
            Ok(seed) => Ok(ScanItemSeed::Tags(Some(seed))),
            Err(error) => {
                tracing::warn!(
                    "{folder} keeps no reading of its file tags, and starts on a blank draft \
                     if it has none: they could not be read: {error}"
                );
                Ok(ScanItemSeed::Tags(None))
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
        let seed = self
            .scan_item_seed(
                item,
                generation,
                std::sync::Arc::new(crate::import::folder_scanner::FileReads::new(
                    std::sync::Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
                )),
            )
            .await?;
        let writes = self
            .save_folder_scan_items(
                watched_folder_path,
                generation,
                vec![crate::db::ScanItemToWrite {
                    item: item.clone(),
                    seed,
                    folder_date: None,
                }],
            )
            .await?;
        Ok(writes.map(|mut writes| writes.pop().expect("one item stores one write").1))
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

    pub async fn load_releases_reading(
        &self,
        content_hash: &str,
        file_edit_revision: u64,
    ) -> Result<Vec<(String, crate::import::folder_scanner::CategorizedFiles)>, LibraryError> {
        Ok(self
            .database
            .load_releases_reading(content_hash, file_edit_revision)
            .await?)
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

    /// The Discogs identities of the artists an import of `credits` would
    /// create, whose pictures it fetches before it writes anything. An artist
    /// the credits resolve to in the library keeps its own picture, so it is
    /// not among them.
    pub(crate) async fn new_discogs_artists(
        &self,
        credits: &[DbArtist],
        picked: &[String],
        content_hash: Option<&str>,
    ) -> Result<std::collections::BTreeSet<String>, crate::import::ImportError> {
        let resolved = self
            .database
            .resolve_artists(credits, picked, content_hash)
            .await
            .map_err(LibraryError::from)?;
        Ok(resolved
            .inserts
            .iter()
            .filter_map(|artist| artist.discogs_artist_id.clone())
            .collect())
    }

    /// Insert all of an import's data in one transaction, so the release exists
    /// complete or not at all. A Remote import stages its files for the cloud
    /// in the same write, kept here once stored when it asks for that.
    ///
    /// The pictures of the artists the import creates are staged from
    /// `artist_images`, the ones fetched for [`Self::new_discogs_artists`],
    /// against a read just before the write; the write fails if its own
    /// artist resolution creates different artists, so no picture lands
    /// without its artist.
    pub(crate) async fn finalize_import_atomic(
        &self,
        guard: crate::db::ImportCommitGuard,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        rows: crate::db::ImportRows<'_>,
        files: Vec<crate::import::service::PreparedImportFile>,
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: &[crate::import::DiscogsArtistImage],
    ) -> Result<(), LibraryError> {
        let expected = self
            .database
            .resolve_artists(
                rows.artists.credits,
                rows.artists.picked,
                guard.content_hash(),
            )
            .await?;
        let expected_new_artists: Vec<String> = expected
            .inserts
            .iter()
            .map(|artist| artist.id.clone())
            .collect();
        let artist_images = self.artist_image_rows(&expected.inserts, artist_images)?;
        let artist_images: Vec<_> = artist_images
            .iter()
            .map(|(image, bytes)| (image, bytes.as_slice()))
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
            )
            .await?;
        Ok(())
    }

    /// Every stored candidate row, keyed by content hash, for the tests that
    /// read back what identification stored.
    #[cfg(test)]
    pub(crate) async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self.database.load_import_candidate_states().await?)
    }

    /// Pick the library album the candidate `content_hash` joins, or
    /// (`None`) leave it to its records.
    pub(crate) async fn set_candidate_album_pick(
        &self,
        content_hash: &str,
        album_id: Option<&str>,
    ) -> Result<(), crate::import::ImportError> {
        self.database
            .set_candidate_album_pick(content_hash, album_id)
            .await
            .map_err(|error| crate::import::ImportError::Db(LibraryError::Database(error)))
    }

    /// The library album picked for the candidate `content_hash`, if one is.
    pub(crate) async fn candidate_album_pick(
        &self,
        content_hash: &str,
    ) -> Result<Option<String>, crate::import::ImportError> {
        self.database
            .candidate_album_pick(content_hash)
            .await
            .map_err(|error| crate::import::ImportError::Db(LibraryError::Database(error)))
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

    /// Whether a verdict is stored for the files at `content_hash` as their
    /// file decisions stand at `edit_revision`.
    pub(crate) async fn verdict_stored_for(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Result<bool, LibraryError> {
        Ok(self
            .database
            .verdict_stored_for(content_hash, edit_revision)
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

    /// Record that the import `import_id` of the candidate at
    /// `candidate_key` failed, ending its claim in the same write.
    pub(crate) async fn fail_candidate_import(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &crate::import::ImportFailure,
        candidate_key: &str,
        import_id: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .fail_candidate_import(
                content_hash,
                edit_revision,
                failure,
                candidate_key,
                import_id,
            )
            .await?)
    }

    /// Remove every candidate's standing, as the library opens.
    pub(crate) async fn clear_candidate_standings(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_candidate_standings().await?)
    }

    /// What the scans of `watched_folder_path` stored about each file they
    /// read, for the next pass over it to start from, reading any other
    /// track's tags through `tags`.
    pub(crate) async fn load_file_reads(
        &self,
        watched_folder_path: &str,
        tags: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> Result<crate::import::folder_scanner::FileReads, LibraryError> {
        Ok(self
            .database
            .load_file_reads(watched_folder_path, tags)
            .await?)
    }

    /// Every candidate's user-set file decisions, keyed by content hash, so a
    /// folder scan reports the roles the user settled.
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

    /// Store the reading a scan settled on for one folder, never replacing the
    /// user's own answer.
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

    /// The grouping the folder `folder` is read as; `None` when no reading
    /// of it is stored.
    pub(crate) async fn folder_reading_grouping(
        &self,
        folder: &crate::import::folder_scanner::FolderReleaseDecisionKey,
    ) -> Result<Option<String>, LibraryError> {
        Ok(self.database.folder_reading_grouping(folder).await?)
    }

    /// Whether reading the folder at `key` as `decision` is something the
    /// stored scan offers.
    pub(crate) async fn folder_reading_offered(
        &self,
        key: &crate::import::folder_scanner::FolderReleaseDecisionKey,
        decision: crate::import::folder_scanner::FolderReleaseDecision,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.folder_reading_offered(key, decision).await?)
    }

    /// The keys of the scanned releases at `folder` or below it.
    pub(crate) async fn scanned_release_keys_at_or_below(
        &self,
        watched_folder_path: &str,
        folder: &str,
    ) -> Result<std::collections::BTreeSet<String>, LibraryError> {
        Ok(self
            .database
            .scanned_release_keys_at_or_below(watched_folder_path, folder)
            .await?)
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
