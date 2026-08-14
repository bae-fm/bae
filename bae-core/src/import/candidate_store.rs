use super::folder_registry::{ImportFolderRegistry, WatchedFolder};
use super::folder_scanner::{
    CategorizedFiles, FolderCandidate, FolderReleaseBoundary, FolderReleaseDecision,
    FolderReleaseDecisionKey, InvalidCandidate, ScanItem,
};
use super::handle::ImportEvent;
use super::types::{ImportProgress, ImportStep, PrepareStep};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

mod state;

use state::CandidateState;
pub use state::{
    CandidateImportStatusSnapshot, CandidateRuntimeSnapshot, FolderImportCandidateSnapshot,
    FolderScanStatus, ImportCandidateSnapshot, ImportCandidatesSnapshot,
    RuntimeImportCandidateSnapshot, WatchedFolderScanStatus,
};

/// The synchronized owner of the import scanner's in-memory projection.
///
/// Clones share one private state value. Callers can perform candidate
/// operations, but cannot obtain the state or its lock.
#[derive(Clone, Default)]
pub(super) struct CandidateStore {
    state: Arc<Mutex<CandidateState>>,
}

impl CandidateStore {
    pub(super) fn restore(
        snapshots: Vec<crate::db::DbFolderScanSnapshot>,
        watched_roots: &HashSet<String>,
        folder_registry: &ImportFolderRegistry,
        imported_content_hashes: &HashSet<String>,
    ) -> Result<Self, crate::import::ImportError> {
        let mut state = CandidateState::default();
        state.restore_folder_scans(
            snapshots,
            watched_roots,
            folder_registry,
            imported_content_hashes,
        )?;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub(super) fn begin_root_scan(&self, root: &Path, generation: u64) {
        self.state.lock().unwrap().begin_root_scan(root, generation);
    }

    pub(super) fn begin_root_scan_with_release_decisions(
        &self,
        root: &Path,
        generation: u64,
        decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
    ) -> Vec<String> {
        let mut state = self.state.lock().unwrap();
        state.begin_root_scan(root, generation);
        state.apply_release_decisions(decisions)
    }

    pub(super) fn persisted_removals_for_item(&self, root: &Path, item: &ScanItem) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .persisted_removals_for_item(root, item)
    }

    pub(super) fn apply_scan_item_if_current(
        &self,
        root: &Path,
        generation: u64,
        item: ScanItem,
        skipped: bool,
        is_added: bool,
    ) -> Option<Vec<String>> {
        self.state
            .lock()
            .unwrap()
            .apply_scan_item_if_current(root, generation, item, skipped, is_added)
    }

    pub(super) fn generation_is_current(&self, root: &Path, generation: u64) -> bool {
        self.state
            .lock()
            .unwrap()
            .generation_is_current(root, generation)
    }

    pub(super) fn fail_root_scan(&self, root: &Path, generation: u64, error: String) -> bool {
        self.state
            .lock()
            .unwrap()
            .fail_root_scan(root, generation, error)
    }

    pub(super) fn finish_root_scan(
        &self,
        root: &Path,
        generation: u64,
        candidate_keys: &HashSet<String>,
        boundary_keys: &HashSet<FolderReleaseDecisionKey>,
    ) -> Option<Vec<String>> {
        self.state
            .lock()
            .unwrap()
            .finish_root_scan(root, generation, candidate_keys, boundary_keys)
    }

    pub(super) fn remove_root(&self, root: &Path) -> Vec<String> {
        self.state.lock().unwrap().remove_root(root)
    }

    pub(super) fn release_boundary_ancestor_keys(
        &self,
        key: &FolderReleaseDecisionKey,
    ) -> Option<Vec<FolderReleaseDecisionKey>> {
        self.state
            .lock()
            .unwrap()
            .release_boundary_ancestor_keys(key)
    }

    pub(super) fn record_event(&self, event: &ImportEvent) {
        self.state.lock().unwrap().record_event(event);
    }

    pub(super) fn snapshot(&self, watched_folders: Vec<WatchedFolder>) -> ImportCandidatesSnapshot {
        self.state.lock().unwrap().snapshot(watched_folders)
    }

    pub(super) fn get(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        self.state.lock().unwrap().get(key)
    }

    pub(super) fn claim_for_import(&self, candidate_key: &str) {
        self.state.lock().unwrap().claim_for_import(candidate_key);
    }

    pub(super) fn release_import_claim(&self, candidate_key: &str) {
        self.state
            .lock()
            .unwrap()
            .release_import_claim(candidate_key);
    }

    pub(super) fn set_skipped(&self, key: &str, skipped: bool) {
        self.state.lock().unwrap().set_skipped(key, skipped);
    }

    pub(super) fn files_for_identity(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Vec<(String, CategorizedFiles)> {
        self.state
            .lock()
            .unwrap()
            .files_for_identity(content_hash, edit_revision)
    }

    pub(super) fn set_files_for_identity(
        &self,
        expected_content_hash: &str,
        expected_edit_revision: u64,
        settled: Vec<(String, CategorizedFiles)>,
        next_edit_revision: u64,
    ) -> Option<Vec<FolderCandidate>> {
        self.state.lock().unwrap().set_files_for_identity(
            expected_content_hash,
            expected_edit_revision,
            settled,
            next_edit_revision,
        )
    }
}
