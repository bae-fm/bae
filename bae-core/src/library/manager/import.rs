//! Import domain operations for [`LibraryManager`].

use super::*;
use crate::import::CandidateKey;

impl LibraryManager {
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

    pub(crate) async fn load_import_candidate(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<crate::import::ImportCandidateDetailProjection>, LibraryError> {
        Ok(self.database.load_import_candidate(key).await?)
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

    /// Every stored candidate row, keyed by content hash, for the tests that
    /// read back what identification stored.
    #[cfg(test)]
    pub(crate) async fn load_import_candidate_states(
        &self,
    ) -> Result<std::collections::HashMap<String, crate::db::DbImportCandidateState>, LibraryError>
    {
        Ok(self.database.load_import_candidate_states().await?)
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbImportCandidateState>, LibraryError> {
        Ok(self.database.load_import_candidate_state(content_hash).await?)
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

    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::folder_scanner::CandidateFileEdits, LibraryError> {
        Ok(self.database.load_candidate_file_edits(content_hash).await?)
    }
}
