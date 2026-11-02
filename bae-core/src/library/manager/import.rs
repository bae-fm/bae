//! Import domain operations for [`LibraryManager`].

use super::*;
use crate::import::CandidateKey;

impl LibraryManager {
    pub(crate) async fn set_grouping_skipped(
        &self,
        key: &CandidateKey,
        skipped: bool,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.set_grouping_skipped(key, skipped).await?)
    }

    /// The release stored at `key`, when it may be worked on — or why a
    /// grouping's release cannot be.
    pub(crate) async fn load_release_candidate(
        &self,
        key: &CandidateKey,
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
        key: CandidateKey,
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
        key: &CandidateKey,
    ) -> Result<
        (Vec<crate::import::folder_scanner::ScanItem>, crate::db::GroupingChanges),
        LibraryError,
    > {
        Ok(self.database.separate_picked_grouping(key).await?)
    }

    /// How the grouping `key` reads, or `None` when no grouping has it.
    pub(crate) async fn load_grouping(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<crate::db::GroupingFacts>, LibraryError> {
        Ok(self.database.load_grouping(key).await?)
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

    pub(crate) fn subscribe_import_counts(&self) -> coven::LiveQuery<crate::import::ImportCounts> {
        self.database.subscribe_import_counts()
    }

    pub(crate) fn subscribe_candidate_runtime(
        &self,
        key: &crate::import::CandidateKey,
    ) -> coven::LiveQuery<Option<crate::import::CandidateRuntimeSnapshot>> {
        self.database.subscribe_candidate_runtime(key)
    }

    pub(crate) async fn close_import_lists(&self) -> Result<(), LibraryError> {
        Ok(self.database.close_import_lists().await?)
    }

    pub(crate) fn subscribe_invalid_candidate(
        &self,
        initial: Option<CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<CandidateKey>,
        Option<crate::import::InvalidCandidateDetail>,
    > {
        self.database.subscribe_invalid_candidate(initial)
    }

    pub(crate) fn subscribe_import_candidate(
        &self,
        initial: Option<CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<CandidateKey>,
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
        Ok(self.database.save_import_candidate_session(content_hash, session).await?)
    }

    /// Move the pane of every one of these candidates by `pane_move`.
    pub(crate) async fn move_import_candidate_panes(
        &self,
        content_hashes: Vec<String>,
        pane_move: crate::import::PaneMove,
    ) -> Result<(), LibraryError> {
        Ok(self.database.move_import_candidate_panes(content_hashes, pane_move).await?)
    }

    /// Record what a candidate's identification asks about.
    pub(crate) async fn save_import_candidate_lookup_choices(
        &self,
        content_hash: &str,
        choices: &crate::identify::LookupChoices,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_import_candidate_lookup_choices(content_hash, choices).await?)
    }

    pub(crate) async fn load_import_candidate(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<crate::import::ImportCandidateDetailProjection>, LibraryError> {
        Ok(self.database.load_import_candidate(key).await?)
    }

    /// The stored release `release` names, as import surfaces read it, or
    /// `None` when no row names it.
    pub(crate) async fn load_source_release(
        &self,
        release: &crate::catalog_model::records::LookupRef,
    ) -> Result<Option<crate::catalog_model::source_release::SourceRelease>, LibraryError> {
        Ok(self.database.load_source_release(release).await?)
    }

    /// Store `release` with no row naming it, which only a test's setup
    /// does.
    #[cfg(test)]
    pub(crate) async fn save_source_release(
        &self,
        release: &crate::catalog_model::source_release::SourceRelease,
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
        path: &Path,
    ) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self.database.watched_import_folder_above(path).await?)
    }

    /// The watched folders inside `path`, not `path` itself, in their order.
    pub(crate) async fn watched_import_folders_below(
        &self,
        path: &Path,
    ) -> Result<Vec<PathBuf>, LibraryError> {
        Ok(self.database.watched_import_folders_below(path).await?)
    }

    /// Whether `path`, spelled as the store keys it, is a watched folder.
    pub(crate) async fn is_watched_import_folder(&self, path: &Path) -> Result<bool, LibraryError> {
        Ok(self.database.is_watched_import_folder(path).await?)
    }

    pub(crate) async fn is_release_candidate_skipped(
        &self,
        candidate: &crate::import::folder_scanner::FolderCandidate,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.is_release_candidate_skipped(candidate).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &Path,
    ) -> Result<std::collections::HashSet<crate::import::RelativeFolder>, LibraryError> {
        Ok(self.database.load_skipped_import_candidates(watched_folder_path).await?)
    }

    pub async fn add_watched_import_folder(&self, path: &Path) -> Result<bool, LibraryError> {
        Ok(self.database.add_watched_import_folder(path).await?)
    }

    /// Stop watching `roots`, watching `parent` in their place when given.
    /// See [`crate::db::Database::remove_watched_import_folders`].
    pub async fn remove_watched_import_folders(
        &self,
        roots: Vec<PathBuf>,
        parent: Option<PathBuf>,
    ) -> Result<Option<Vec<CandidateKey>>, LibraryError> {
        Ok(self.database.remove_watched_import_folders(roots, parent).await?)
    }

    pub async fn set_import_candidate_skipped(
        &self,
        candidate_path: &Path,
        skipped: bool,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.set_import_candidate_skipped(candidate_path, skipped).await?)
    }

    /// Open a new scan generation for `watched_folder_path`, recording the
    /// volume it is on.
    pub async fn begin_folder_scan(&self, watched_folder_path: &Path) -> Result<u64, LibraryError> {
        let volume = crate::import::volume::volume_kind(
            std::path::Path::new(watched_folder_path),
            &self.diagnostics,
        )
        .await;
        Ok(self.database.begin_folder_scan(watched_folder_path, volume).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        item: &crate::import::folder_scanner::ScanItem,
    ) -> Result<Option<crate::db::ScanItemWrite>, LibraryError> {
        let seed = crate::import::file_metadata_seed::scan_item_seed(
            &self.database,
            &self.diagnostics,
            item,
            generation,
            std::sync::Arc::new(crate::import::folder_scanner::FileReads::new(
                std::sync::Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
            )),
        )
        .await?;
        let writes = self
            .database
            .save_folder_scan_items(
                watched_folder_path,
                generation,
                vec![crate::db::ScanItemToWrite { item: item.clone(), seed, folder_date: None }],
            )
            .await?;
        Ok(writes.map(|mut writes| writes.pop().expect("one item stores one write").1))
    }

    pub async fn finish_folder_scan(
        &self,
        watched_folder_path: &Path,
        generation: u64,
        error: Option<&str>,
    ) -> Result<Option<crate::db::FinishedScan>, LibraryError> {
        Ok(self.database.finish_folder_scan(watched_folder_path, generation, error).await?)
    }

    #[cfg(test)]
    pub async fn load_folder_scan_snapshots(
        &self,
    ) -> Result<Vec<crate::db::DbFolderScanSnapshot>, LibraryError> {
        Ok(self.database.load_folder_scan_snapshots().await?)
    }

    pub async fn load_folder_scan_items(
        &self,
        watched_folder_path: &Path,
    ) -> Result<Vec<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_folder_scan_items(watched_folder_path).await?)
    }

    pub async fn load_releases_reading(
        &self,
        content_hash: &str,
        file_edit_revision: u64,
    ) -> Result<Vec<(CandidateKey, crate::import::folder_scanner::CategorizedFiles)>, LibraryError>
    {
        Ok(self.database.load_releases_reading(content_hash, file_edit_revision).await?)
    }

    pub async fn load_folder_scan_item(
        &self,
        entry_key: &CandidateKey,
    ) -> Result<Option<crate::import::folder_scanner::ScanItem>, LibraryError> {
        Ok(self.database.load_folder_scan_item(entry_key).await?)
    }

    pub(crate) async fn load_candidate_file_tag_snapshot(
        &self,
        watched_folder_path: &Path,
        candidate_path: &CandidateKey,
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
        Ok(resolved.inserts.iter().filter_map(|artist| artist.discogs_artist_id.clone()).collect())
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
            .resolve_artists(rows.artists.credits, rows.artists.picked, guard.content_hash())
            .await?;
        let expected_new_artists: Vec<String> =
            expected.inserts.iter().map(|artist| artist.id.clone()).collect();
        let artist_images = self.artist_image_rows(&expected.inserts, artist_images)?;
        let artist_images: Vec<_> =
            artist_images.iter().map(|(image, bytes)| (image, bytes.as_slice())).collect();
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
    ) -> Result<std::collections::HashMap<String, crate::db::DbImportCandidateState>, LibraryError>
    {
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
        Ok(self.database.load_import_candidate_state(content_hash).await?)
    }

    /// Whether a verdict is stored for the files at `content_hash` as their
    /// file decisions stand at `edit_revision`.
    pub(crate) async fn verdict_stored_for(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Result<bool, LibraryError> {
        Ok(self.database.verdict_stored_for(content_hash, edit_revision).await?)
    }

    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<crate::db::DbCandidatePaneRows, LibraryError> {
        Ok(self.database.load_import_candidate_pane_rows(content_hash).await?)
    }

    /// The rows an import of the candidate `content_hash` consumes. A
    /// candidate whose files this device already imported holds no draft, so
    /// it fails as an import of those files does.
    pub async fn load_import_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbCandidateImportPreparation>, LibraryError> {
        match self.database.load_import_candidate_preparation(content_hash).await? {
            None => Ok(None),
            Some(crate::db::DbPreparationRead::Prepared(preparation)) => Ok(Some(preparation)),
            Some(crate::db::DbPreparationRead::Imported { release_id }) => {
                Err(LibraryError::Import(Box::new(
                    crate::import::ImportError::FilesAlreadyImported { release_id },
                )))
            }
        }
    }

    pub async fn load_import_candidate_prepared_assets(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::CandidatePreparedAssets, LibraryError> {
        Ok(self.database.load_import_candidate_prepared_assets(content_hash).await?)
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
        candidate_key: &CandidateKey,
        import_id: &str,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .fail_candidate_import(content_hash, edit_revision, failure, candidate_key, import_id)
            .await?)
    }

    /// Remove every candidate's standing, as the library opens.
    pub(crate) async fn clear_candidate_standings(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_candidate_standings().await?)
    }

    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::folder_scanner::CandidateFileEdits, LibraryError> {
        Ok(self.database.load_candidate_file_edits(content_hash).await?)
    }

    /// The grouping the folder `folder` is read as; `None` when no reading
    /// of it is stored.
    pub(crate) async fn folder_reading_grouping(
        &self,
        folder: &crate::import::folder_scanner::FolderReleaseDecisionKey,
    ) -> Result<Option<CandidateKey>, LibraryError> {
        Ok(self.database.folder_reading_grouping(folder).await?)
    }

    /// The keys of the scanned releases at `folder` or below it.
    pub(crate) async fn scanned_release_keys_at_or_below(
        &self,
        watched_folder_path: &Path,
        folder: &Path,
    ) -> Result<std::collections::BTreeSet<CandidateKey>, LibraryError> {
        Ok(self.database.scanned_release_keys_at_or_below(watched_folder_path, folder).await?)
    }
}
