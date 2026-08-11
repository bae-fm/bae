use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseBoundary, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate,
};
use crate::import::types::{
    ImportCommand, ImportProgress, ImportStep, MetadataSource, StorageMode,
};
use crate::library::manager::discogs_validation_from_result as validation_from_validate_result;
use crate::library::LibraryManager;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

mod import;
mod scan;
mod search;
mod watch;

#[cfg(test)]
mod tests;

/// Send an import event on the broadcast bus, logging on send failure.
/// `broadcast::Sender::send` returns `Err` only when there are zero active
/// receivers — a warning is appropriate (something upstream lost interest)
/// but not fatal.
pub(super) fn send_event(sender: &broadcast::Sender<ImportEvent>, ev: ImportEvent) {
    if let Err(e) = sender.send(ev) {
        warn!("import event send failed: {}", e);
    }
}

#[derive(Debug, Clone)]
pub struct ImportCandidatesSnapshot {
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_candidates: Vec<FolderImportCandidateSnapshot>,
    pub invalid_candidates: Vec<InvalidCandidate>,
    pub boundaries: Vec<FolderReleaseBoundary>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
}

#[derive(Debug, Clone)]
pub struct WatchedFolderScanStatus {
    pub watched_folder_path: String,
    pub watched_folder_name: String,
    pub status: FolderScanStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderScanStatus {
    Scanning,
    Complete,
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct FolderImportCandidateSnapshot {
    pub candidate: FolderCandidate,
    pub runtime: CandidateRuntimeSnapshot,
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
}

#[derive(Debug, Clone)]
pub enum ImportCandidateSnapshot {
    Folder {
        candidate: FolderCandidate,
        runtime: CandidateRuntimeSnapshot,
        actionable: bool,
        skipped: bool,
        is_added: bool,
    },
    Invalid(InvalidCandidate),
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}

#[derive(Debug, Clone)]
enum ScannedCandidate {
    Folder {
        candidate: FolderCandidate,
        actionable: bool,
        skipped: bool,
        is_added: bool,
    },
    Invalid(InvalidCandidate),
}

#[derive(Debug, Clone)]
pub struct CandidateRuntimeSnapshot {
    pub identify_state: crate::identify::IdentifyState,
    pub toolbar: Vec<crate::identify::ToolbarSignal>,
    pub signals: Option<crate::signals::Signals>,
    pub import_status: Option<CandidateImportStatusSnapshot>,
}

impl CandidateRuntimeSnapshot {
    fn idle() -> Self {
        Self {
            identify_state: crate::identify::IdentifyState::Idle,
            toolbar: Vec::new(),
            signals: None,
            import_status: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CandidateImportStatusSnapshot {
    Importing {
        progress_percent: u32,
        step: Option<ImportStep>,
    },
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: String,
    },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ImportCandidateState {
    /// What the folder scanner last reported per candidate path.
    candidates: HashMap<String, ScannedCandidate>,
    /// Per-candidate-key runtime accumulated from recorded events (identify
    /// state, signals, import progress). A key without an entry has had no
    /// events; reads treat absence as the idle runtime. Also holds
    /// `reidentify:`-prefixed keys, which have no scanned candidate.
    runtime: HashMap<String, CandidateRuntimeSnapshot>,
    boundaries: HashMap<FolderReleaseDecisionKey, FolderReleaseBoundary>,
    folder_scan_statuses: HashMap<String, FolderScanStatus>,
    root_generations: HashMap<String, u64>,
}

impl ImportCandidateState {
    const REIDENTIFY_PREFIX: &str = "reidentify:";

    fn candidate_watched_folder_path(candidate: &ScannedCandidate) -> &str {
        match candidate {
            ScannedCandidate::Folder { candidate, .. } => &candidate.watched_folder_path,
            ScannedCandidate::Invalid(candidate) => &candidate.watched_folder_path,
        }
    }

    fn runtime_entry(&mut self, candidate_key: &str) -> &mut CandidateRuntimeSnapshot {
        self.runtime
            .entry(candidate_key.to_string())
            .or_insert_with(CandidateRuntimeSnapshot::idle)
    }

    fn same_folder_shape(left: &FolderCandidate, right: &FolderCandidate) -> bool {
        left.files.content_hash() == right.files.content_hash()
            && left.file_edit_revision == right.file_edit_revision
            && left.scope == right.scope
            && left.file_root == right.file_root
    }

    /// Runtime for a candidate key at read time. Absence from the runtime map
    /// means no identify/import events have been recorded for the key — the
    /// designed initial state — so it reads as idle.
    fn runtime_for(&self, key: &str) -> CandidateRuntimeSnapshot {
        self.runtime
            .get(key)
            .cloned()
            .unwrap_or_else(CandidateRuntimeSnapshot::idle)
    }

    /// Record one actionable candidate and return every candidate key whose
    /// prior shape this result superseded. An unchanged re-scan preserves its
    /// runtime; a changed file set or release boundary clears it before the new
    /// shape becomes visible.
    pub(super) fn upsert_folder(
        &mut self,
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    ) -> Vec<String> {
        let key = candidate.path.to_string_lossy().into_owned();
        let mut superseded = Vec::new();
        if let Some(ScannedCandidate::Folder {
            candidate: existing,
            ..
        }) = self.candidates.get(&key)
        {
            let same_shape = Self::same_folder_shape(existing, &candidate);
            if !same_shape {
                superseded.push(key.clone());
                self.runtime.remove(&key);
            }
        }

        for resolved in &candidate.resolved_boundaries {
            self.boundaries.remove(&resolved.key);
            let boundary_path = Path::new(&resolved.key.watched_folder_path)
                .join(&resolved.key.relative_folder_path);
            let boundary_key = boundary_path.to_string_lossy();
            let replaced: Vec<_> = self
                .candidates
                .iter()
                .filter_map(|(existing_key, existing)| {
                    if existing_key == &key
                        || Self::candidate_watched_folder_path(existing)
                            != resolved.key.watched_folder_path
                    {
                        return None;
                    }
                    let existing_path = Path::new(existing_key);
                    let is_replaced = match resolved.decision {
                        FolderReleaseDecision::CombineAsOneRelease => {
                            existing_path.starts_with(&boundary_path)
                        }
                        FolderReleaseDecision::KeepAsSeparateReleases => {
                            existing_key == boundary_key.as_ref()
                        }
                    };
                    is_replaced.then(|| existing_key.clone())
                })
                .collect();
            for replaced_key in replaced {
                self.candidates.remove(&replaced_key);
                self.runtime.remove(&replaced_key);
                superseded.push(replaced_key);
            }
        }
        self.candidates.insert(
            key,
            ScannedCandidate::Folder {
                candidate,
                actionable: true,
                skipped,
                is_added,
            },
        );
        superseded.sort();
        superseded.dedup();
        superseded
    }

    pub(super) fn upsert_discovered(
        &mut self,
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    ) -> bool {
        let key = candidate.path.to_string_lossy().into_owned();
        let contradicted_actionable = matches!(
            self.candidates.get(&key),
            Some(ScannedCandidate::Folder {
                candidate: existing,
                actionable: true,
                ..
            }) if !Self::same_folder_shape(existing, &candidate)
        );
        if matches!(
            self.candidates.get(&key),
            Some(ScannedCandidate::Folder {
                candidate: existing,
                actionable: true,
                ..
            }) if Self::same_folder_shape(existing, &candidate)
        ) {
            return false;
        }
        if contradicted_actionable {
            self.runtime.remove(&key);
        }
        self.candidates.insert(
            key,
            ScannedCandidate::Folder {
                candidate,
                actionable: false,
                skipped,
                is_added,
            },
        );
        contradicted_actionable
    }

    /// [`Self::upsert_folder`] for a folder that failed validation.
    pub(super) fn upsert_invalid(&mut self, candidate: InvalidCandidate) -> bool {
        let key = candidate.path.to_string_lossy().into_owned();
        for resolved in &candidate.resolved_boundaries {
            self.boundaries.remove(&resolved.key);
        }
        let replaced_folder = matches!(
            self.candidates.get(&key),
            Some(ScannedCandidate::Folder { .. })
        );
        if replaced_folder {
            self.runtime.remove(&key);
        }
        self.candidates
            .insert(key, ScannedCandidate::Invalid(candidate));
        replaced_folder
    }

    pub(super) fn upsert_boundary(&mut self, boundary: FolderReleaseBoundary) -> Vec<String> {
        let removed = boundary.candidate_keys.clone();
        for key in &removed {
            self.candidates.remove(key);
            self.runtime.remove(key);
        }
        self.boundaries.insert(boundary.key.clone(), boundary);
        removed
    }

    pub(super) fn apply_release_decisions(
        &mut self,
        decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
    ) -> Vec<String> {
        let persisted_keys: HashSet<String> = decisions
            .iter()
            .flat_map(|(key, _)| self.persisted_keys_for_root(Path::new(&key.watched_folder_path)))
            .collect();
        let removed = crate::import::folder_scanner::release_decision_removed_keys(
            &persisted_keys,
            decisions,
        );
        for (key, _) in decisions {
            self.boundaries.remove(key);
        }
        for candidate_key in &removed {
            self.candidates.remove(candidate_key);
            self.runtime.remove(candidate_key);
        }
        removed
    }

    pub(super) fn begin_root_scan(&mut self, root: &Path, generation: u64) {
        let root = root.to_string_lossy().into_owned();
        self.root_generations.insert(root.clone(), generation);
        self.folder_scan_statuses
            .insert(root, FolderScanStatus::Scanning);
    }

    fn persisted_keys_for_root(&self, root: &Path) -> HashSet<String> {
        let root = root.to_string_lossy();
        self.candidates
            .iter()
            .filter(|(_, candidate)| {
                Self::candidate_watched_folder_path(candidate) == root.as_ref()
            })
            .map(|(key, _)| key.clone())
            .chain(
                self.boundaries
                    .keys()
                    .filter(|key| key.watched_folder_path == root.as_ref())
                    .map(|key| {
                        Path::new(&key.watched_folder_path)
                            .join(&key.relative_folder_path)
                            .to_string_lossy()
                            .into_owned()
                    }),
            )
            .collect()
    }

    pub(super) fn persisted_removals_for_item(
        &self,
        root: &Path,
        item: &crate::import::folder_scanner::ScanItem,
    ) -> Vec<String> {
        let before = self.persisted_keys_for_root(root);
        let mut after = self.clone();
        after.apply_scan_item(item.clone(), false, false);
        let after = after.persisted_keys_for_root(root);
        before.difference(&after).cloned().collect()
    }

    pub(super) fn apply_scan_item(
        &mut self,
        item: crate::import::folder_scanner::ScanItem,
        skipped: bool,
        is_added: bool,
    ) -> Vec<String> {
        match item {
            crate::import::folder_scanner::ScanItem::Discovered(candidate) => {
                let key = candidate.path.to_string_lossy().into_owned();
                self.upsert_discovered(candidate, skipped, is_added)
                    .then_some(key)
                    .into_iter()
                    .collect()
            }
            crate::import::folder_scanner::ScanItem::Valid(candidate) => {
                self.upsert_folder(candidate, skipped, is_added)
            }
            crate::import::folder_scanner::ScanItem::Invalid(candidate) => {
                let key = candidate.path.to_string_lossy().into_owned();
                self.upsert_invalid(candidate)
                    .then_some(key)
                    .into_iter()
                    .collect()
            }
            crate::import::folder_scanner::ScanItem::Boundary(boundary) => {
                self.upsert_boundary(boundary)
            }
        }
    }

    pub(super) fn apply_scan_item_if_current(
        &mut self,
        root: &Path,
        generation: u64,
        item: crate::import::folder_scanner::ScanItem,
        skipped: bool,
        is_added: bool,
    ) -> Option<Vec<String>> {
        self.generation_is_current(root, generation)
            .then(|| self.apply_scan_item(item, skipped, is_added))
    }

    pub(super) fn restore_folder_scans(
        &mut self,
        snapshots: Vec<crate::db::DbFolderScanSnapshot>,
        watched_roots: &HashSet<String>,
        folder_registry: &ImportFolderRegistry,
        imported_content_hashes: &HashSet<String>,
    ) -> Result<(), crate::import::ImportError> {
        for snapshot in snapshots {
            if !watched_roots.contains(&snapshot.watched_folder_path) {
                continue;
            }
            self.root_generations
                .insert(snapshot.watched_folder_path.clone(), snapshot.generation);
            self.folder_scan_statuses
                .insert(snapshot.watched_folder_path, snapshot.status);
            for item in snapshot.items {
                let (skipped, is_added) = match &item {
                    crate::import::folder_scanner::ScanItem::Discovered(candidate)
                    | crate::import::folder_scanner::ScanItem::Valid(candidate) => (
                        folder_registry
                            .is_skipped(&candidate.watched_folder_path, &candidate.path)?,
                        imported_content_hashes.contains(&candidate.files.content_hash()),
                    ),
                    crate::import::folder_scanner::ScanItem::Invalid(_)
                    | crate::import::folder_scanner::ScanItem::Boundary(_) => (false, false),
                };
                self.apply_scan_item(item, skipped, is_added);
            }
        }
        Ok(())
    }

    pub(super) fn generation_is_current(&self, root: &Path, generation: u64) -> bool {
        self.root_generations.get(root.to_string_lossy().as_ref()) == Some(&generation)
    }

    pub(super) fn fail_root_scan(&mut self, root: &Path, generation: u64, error: String) -> bool {
        if !self.generation_is_current(root, generation) {
            return false;
        }
        self.folder_scan_statuses.insert(
            root.to_string_lossy().into_owned(),
            FolderScanStatus::Failed { error },
        );
        true
    }

    /// Drop every candidate under `root` whose key is not in `keep`, returning
    /// the removed keys (and their runtime) so the caller can announce them.
    ///
    /// The completed-scan counterpart to the upserts above: a walk records each
    /// candidate as it finds it, then calls this once with everything it saw, so
    /// only paths that have genuinely dropped out of the tree are removed. Runs
    /// only on a walk that finished — a failed one reports no keys and must not
    /// be read as "these folders are gone".
    pub(super) fn retain_root(&mut self, root: &Path, keep: &HashSet<String>) -> Vec<String> {
        let root_key = root.to_string_lossy();
        let removed: Vec<String> = self
            .candidates
            .iter()
            .filter(|(key, candidate)| {
                Self::candidate_watched_folder_path(candidate) == root_key.as_ref()
                    && !keep.contains(key.as_str())
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in &removed {
            self.candidates.remove(key);
            self.runtime.remove(key);
        }
        removed
    }

    pub(super) fn retain_root_boundaries(
        &mut self,
        root: &Path,
        keep: &HashSet<FolderReleaseDecisionKey>,
    ) {
        let root = root.to_string_lossy();
        self.boundaries
            .retain(|key, _| key.watched_folder_path != root.as_ref() || keep.contains(key));
    }

    pub(super) fn finish_root_scan(
        &mut self,
        root: &Path,
        generation: u64,
        candidate_keys: &HashSet<String>,
        boundary_keys: &HashSet<FolderReleaseDecisionKey>,
    ) -> Option<Vec<String>> {
        if !self.generation_is_current(root, generation) {
            return None;
        }
        let removed = self.retain_root(root, candidate_keys);
        self.retain_root_boundaries(root, boundary_keys);
        self.folder_scan_statuses.insert(
            root.to_string_lossy().into_owned(),
            FolderScanStatus::Complete,
        );
        Some(removed)
    }

    /// Drop every candidate under `root` (and its runtime), returning the
    /// removed candidate keys so the caller can announce them on the bus.
    pub(super) fn remove_root(&mut self, root: &Path) -> Vec<String> {
        let root_key = root.to_string_lossy();
        let removed: Vec<String> = self
            .candidates
            .iter()
            .filter(|(_, candidate)| {
                Self::candidate_watched_folder_path(candidate) == root_key.as_ref()
            })
            .map(|(key, _)| key.clone())
            .collect();
        self.candidates.retain(|_, candidate| {
            Self::candidate_watched_folder_path(candidate) != root_key.as_ref()
        });
        for key in &removed {
            self.runtime.remove(key);
        }
        self.boundaries
            .retain(|key, _| key.watched_folder_path != root_key.as_ref());
        self.folder_scan_statuses.remove(root_key.as_ref());
        self.root_generations.remove(root_key.as_ref());
        removed
    }

    /// Record that an import owns this candidate.
    ///
    /// Written when the import command is queued, not when the worker's first
    /// `ImportProgress` comes back through [`Self::record_event`]. That event
    /// records the same fact, but far too late to gate anything on: it is
    /// emitted after the worker has dequeued the command and re-walked the
    /// folder — behind however many imports are already queued ahead of it —
    /// and it only reaches this state when the UI event bus drains it. The
    /// queue sweep reads this field to decide whether a candidate still wants
    /// a verdict, and "the user has committed to importing it" has to be true
    /// here from the moment they commit.
    pub(super) fn claim_for_import(&mut self, candidate_key: &str) {
        self.runtime_entry(candidate_key).import_status =
            Some(CandidateImportStatusSnapshot::Importing {
                progress_percent: 0,
                step: None,
            });
    }

    /// Undo [`Self::claim_for_import`] for a command that never made it onto
    /// the worker's queue.
    pub(super) fn release_import_claim(&mut self, candidate_key: &str) {
        self.runtime_entry(candidate_key).import_status = None;
    }

    pub(super) fn set_skipped(&mut self, key: &str, skipped: bool) {
        if let Some(ScannedCandidate::Folder {
            skipped: candidate_skipped,
            ..
        }) = self.candidates.get_mut(key)
        {
            *candidate_skipped = skipped;
        }
    }

    pub(super) fn release_boundary_ancestor_keys(
        &self,
        key: &FolderReleaseDecisionKey,
    ) -> Option<Vec<FolderReleaseDecisionKey>> {
        if self.boundaries.contains_key(key)
            || self.candidates.values().any(|candidate| {
                matches!(
                    candidate,
                    ScannedCandidate::Folder { candidate, .. }
                        if candidate.resolved_boundaries.iter().any(|resolved| resolved.key == *key)
                ) || matches!(
                    candidate,
                    ScannedCandidate::Invalid(candidate)
                        if candidate.resolved_boundaries.iter().any(|resolved| resolved.key == *key)
                )
            })
        {
            return Some(Vec::new());
        }
        let grouped_candidates = self
            .candidates
            .values()
            .filter(|candidate| {
                let (watched_folder_path, display_path) = match candidate {
                    ScannedCandidate::Folder { candidate, .. } => (
                        candidate.watched_folder_path.as_str(),
                        candidate.display_path.as_str(),
                    ),
                    ScannedCandidate::Invalid(candidate) => (
                        candidate.watched_folder_path.as_str(),
                        candidate.display_path.as_str(),
                    ),
                };
                watched_folder_path == key.watched_folder_path
                    && display_path
                        .split('/')
                        .next()
                        .is_some_and(|first| first == key.relative_folder_path)
            })
            .count();
        let grouped_boundaries = self.boundaries.values().any(|boundary| {
            boundary.key.watched_folder_path == key.watched_folder_path
                && boundary
                    .display_path
                    .split('/')
                    .next()
                    .is_some_and(|first| first == key.relative_folder_path)
        });
        if !key.relative_folder_path.contains('/')
            && (grouped_candidates >= 1 || grouped_boundaries)
        {
            return Some(Vec::new());
        }
        let row = self
            .boundaries
            .values()
            .flat_map(|boundary| &boundary.tree_rows)
            .find(|row| row.decision_key == *key)?;
        Some(row.ancestor_decision_keys.clone())
    }

    pub(super) fn files_for_identity(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Vec<(String, crate::import::folder_scanner::CategorizedFiles)> {
        self.candidates
            .iter()
            .filter_map(|(key, scanned)| match scanned {
                ScannedCandidate::Folder { candidate, .. }
                    if candidate.files.content_hash() == content_hash
                        && candidate.file_edit_revision == edit_revision =>
                {
                    Some((key.clone(), candidate.files.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Advance every path that shares one stored file-decision identity. The
    /// caller derives all replacement file sets before the database write and
    /// holds the folder-state commit lock, so this either applies the exact
    /// matching set or reports an invariant break.
    pub(super) fn set_files_for_identity(
        &mut self,
        expected_content_hash: &str,
        expected_edit_revision: u64,
        settled: Vec<(String, crate::import::folder_scanner::CategorizedFiles)>,
        next_edit_revision: u64,
    ) -> Option<Vec<FolderCandidate>> {
        let expected_keys: HashSet<_> = self
            .files_for_identity(expected_content_hash, expected_edit_revision)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        let settled_keys: HashSet<_> = settled.iter().map(|(key, _)| key.clone()).collect();
        if expected_keys.is_empty() || expected_keys != settled_keys {
            return None;
        }
        let mut updated = Vec::with_capacity(settled.len());
        for (key, files) in settled {
            let Some(ScannedCandidate::Folder { candidate, .. }) = self.candidates.get_mut(&key)
            else {
                return None;
            };
            candidate.files = files;
            candidate.file_edit_revision = next_edit_revision;
            self.runtime.remove(&key);
            updated.push(candidate.clone());
        }
        Some(updated)
    }

    pub(super) fn record_event(&mut self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                runtime.import_status = match progress {
                    ImportProgress::Preparing { step, .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: 0,
                            step: Some(ImportStep::Preparing(*step)),
                        })
                    }
                    ImportProgress::Started { .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: 0,
                            step: None,
                        })
                    }
                    ImportProgress::Progress { percent, phase, .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: *percent as u32,
                            step: Some(ImportStep::Running(*phase)),
                        })
                    }
                    ImportProgress::Complete { id, album_id, .. }
                    | ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
                        Some(CandidateImportStatusSnapshot::Complete {
                            release_id: id.clone(),
                            album_id: album_id.clone(),
                        })
                    }
                    ImportProgress::Failed { error, .. } => {
                        Some(CandidateImportStatusSnapshot::Error {
                            error: error.clone(),
                        })
                    }
                };
            }
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar,
                priority: _,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                // A terminal state followed by `Idle` is a driver being torn
                // down after settling — the sweep cancels its own drivers once
                // they settle, and cancellation broadcasts `Idle` on its way
                // out. The candidate's answer doesn't stop being its answer
                // because the machinery that produced it exited, so the
                // terminal state stays. A genuine mid-run cancel goes
                // `Triangulating` → `Idle` and resets as before.
                if !(matches!(state, crate::identify::IdentifyState::Idle)
                    && runtime.identify_state.is_terminal())
                {
                    runtime.identify_state = state.clone();
                    runtime.toolbar = toolbar.clone();
                }
            }
            ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
                priority: _,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                runtime.signals = Some(signals.clone());
            }
            // Queue progress is a queue-wide number with no candidate to record
            // it against; it crosses as an event and is not part of any
            // candidate's runtime.
            ImportEvent::Scan(_)
            | ImportEvent::ImportLoudnessProgress { .. }
            | ImportEvent::QueueIdentifyProgress { .. } => {}
        }
    }

    pub(super) fn snapshot(&self, watched_folders: Vec<WatchedFolder>) -> ImportCandidatesSnapshot {
        let mut watched_order = HashMap::new();
        for (index, folder) in watched_folders.iter().enumerate() {
            watched_order.insert(folder.path.as_str(), index);
        }
        let order_for = |path: &str| match watched_order.get(path) {
            Some(index) => *index,
            None => usize::MAX,
        };

        let mut folder_candidates = Vec::new();
        let mut invalid_candidates = Vec::new();
        for (key, candidate) in &self.candidates {
            match candidate {
                ScannedCandidate::Folder {
                    candidate,
                    actionable,
                    skipped,
                    is_added,
                } => folder_candidates.push((
                    key.as_str(),
                    FolderImportCandidateSnapshot {
                        candidate: candidate.clone(),
                        runtime: self.runtime_for(key),
                        actionable: *actionable,
                        skipped: *skipped,
                        is_added: *is_added,
                    },
                )),
                ScannedCandidate::Invalid(candidate) => {
                    invalid_candidates.push((key.as_str(), candidate.clone()))
                }
            }
        }
        folder_candidates.sort_by(|(_, left), (_, right)| {
            order_for(&left.candidate.watched_folder_path)
                .cmp(&order_for(&right.candidate.watched_folder_path))
                .then_with(|| {
                    natord::compare(&left.candidate.display_path, &right.candidate.display_path)
                })
        });
        invalid_candidates.sort_by(|(_, left), (_, right)| {
            order_for(&left.watched_folder_path)
                .cmp(&order_for(&right.watched_folder_path))
                .then_with(|| natord::compare(&left.display_path, &right.display_path))
        });

        let mut boundaries: Vec<_> = self.boundaries.values().cloned().collect();
        boundaries.sort_by(|left, right| {
            order_for(&left.key.watched_folder_path)
                .cmp(&order_for(&right.key.watched_folder_path))
                .then_with(|| {
                    left.key
                        .relative_folder_path
                        .cmp(&right.key.relative_folder_path)
                })
        });
        let mut folder_scan_statuses: Vec<_> = self
            .folder_scan_statuses
            .iter()
            .map(|(watched_folder_path, status)| {
                let watched_folder = watched_folders
                    .iter()
                    .find(|folder| folder.path == *watched_folder_path)
                    .expect("a scan status belongs to a watched folder");
                WatchedFolderScanStatus {
                    watched_folder_path: watched_folder_path.clone(),
                    watched_folder_name: watched_folder.name.clone(),
                    status: status.clone(),
                }
            })
            .collect();
        folder_scan_statuses.sort_by(|left, right| {
            order_for(&left.watched_folder_path)
                .cmp(&order_for(&right.watched_folder_path))
                .then_with(|| left.watched_folder_path.cmp(&right.watched_folder_path))
        });

        ImportCandidatesSnapshot {
            watched_folders,
            folder_candidates: folder_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
            invalid_candidates: invalid_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
            boundaries,
            folder_scan_statuses,
        }
    }

    pub(super) fn get(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        if key.starts_with(Self::REIDENTIFY_PREFIX) {
            return self.runtime.get(key).cloned().map(|runtime| {
                ImportCandidateSnapshot::Runtime {
                    key: key.to_string(),
                    runtime,
                }
            });
        }
        self.candidates.get(key).map(|candidate| match candidate {
            ScannedCandidate::Folder {
                candidate,
                actionable,
                skipped,
                is_added,
            } => ImportCandidateSnapshot::Folder {
                candidate: candidate.clone(),
                runtime: self.runtime_for(key),
                actionable: *actionable,
                skipped: *skipped,
                is_added: *is_added,
            },
            ScannedCandidate::Invalid(candidate) => {
                ImportCandidateSnapshot::Invalid(candidate.clone())
            }
        })
    }
}

/// All events emitted by the import service. One channel, one subscriber (the bus).
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: String,
        progress: ImportProgress,
    },
    /// Per-track loudness measurement progress for an importing candidate: a
    /// high-frequency tick routed to a native leaf view rather than the candidate
    /// row's coarse step. Separate from `ImportProgress` so it bypasses the
    /// release/import progress subscribers — it carries the candidate key, not a
    /// release or import id.
    ImportLoudnessProgress {
        candidate_key: String,
        tracks_done: u32,
        tracks_total: u32,
        /// Overall scan progress 0..1 for the determinate bar; advances within a
        /// track as it's measured, not just at track boundaries.
        fraction: f32,
    },
    /// Identify pipeline transitioned to a new state. Emitted by the
    /// `identify` module; carries the full state payload plus the pre-shaped
    /// signals toolbar (the interactive badge row) projected from the same
    /// transition, so the UI renders both from one event.
    IdentifyStateChanged {
        candidate_key: String,
        state: crate::identify::IdentifyState,
        toolbar: Vec<crate::identify::ToolbarSignal>,
        /// The run's own priority, carried so a consumer can tell a candidate
        /// a person opened from one the background sweep picked up. The UI bus
        /// re-renders for the first and not the second.
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text), emitted on every transition — extraction start, each
    /// source/OCR completion, natural end, and cancellation. The reducer writes
    /// it wholesale, so it needs no partial-update logic.
    SignalsUpdated {
        candidate_key: String,
        signals: crate::signals::Signals,
        /// The extraction's own priority — same meaning as
        /// [`ImportEvent::IdentifyStateChanged`]'s.
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// How much of the import queue the background sweep has answered. Both
    /// counts are the sweep's own: `total` is how many candidates it is
    /// responsible for, which is a fact about the queue rather than something a
    /// view can infer from the rows it happens to hold.
    QueueIdentifyProgress {
        identified: u32,
        total: u32,
    },
}

impl ImportServiceHandle {
    pub(crate) fn record_candidate_event(&self, event: &ImportEvent) {
        self.candidate_state.lock().unwrap().record_event(event);
    }
}

/// Search query — one of the three search modes.
pub enum SearchQuery {
    General {
        artist: String,
        album: String,
        source: MetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        source: MetadataSource,
    },
    Barcode {
        barcode: String,
        source: MetadataSource,
    },
}

/// Search results grouped by release group, with the per-release library dupe
/// statuses the UI looks up by release id.
#[derive(Debug, Clone)]
pub struct GroupedSearchResults {
    pub groups: Vec<crate::import::release_group::ReleaseGroup>,
    pub statuses: Vec<crate::db::LibraryStatus>,
}

/// What `save_discogs_token` did with a submitted key, after validating against
/// Discogs first.
///
/// - `Valid` — Discogs accepted the key; it's stored and used.
/// - `Unvalidated` — Discogs was unreachable or rate-limited; the key is stored
///   optimistically and will be re-checked when possible.
/// - `Rejected` — Discogs returned 401; nothing is stored, so the UI keeps the
///   draft for the user to correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

/// Handle for sending import requests and subscribing to progress updates.
///
/// A thin orchestration layer that dispatches prefetches, builds
/// `ImportCommand`s carrying just `MetadataRef`s, and forwards them to the
/// worker. It holds no caches of its own — the network-layer caches in
/// `crate::musicbrainz`, `crate::discogs::client`, and the Cover Art Archive
/// client serve every caller transparently.
#[derive(Clone)]
pub struct ImportServiceHandle {
    requests_tx: mpsc::UnboundedSender<crate::import::service::ImportWorkerMessage>,
    /// The worker's OS thread, joined once at teardown (`stop_and_join`). The
    /// thread holds a `LibraryManager` clone, which pins coven's exclusive
    /// store-open lock — until it exits the same library can't reopen
    /// in-process, so teardown must not return before the join. Shared across
    /// handle clones; `take`n by whichever runs the join.
    worker_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    library_manager: LibraryManager,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    /// Unified event channel — all import service events go here.
    event_tx: broadcast::Sender<ImportEvent>,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    candidate_state: Arc<Mutex<ImportCandidateState>>,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
    watcher_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    runtime_handle: tokio::runtime::Handle,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The watched-folder list changed (queried at load, or after add/remove).
    /// Carries the full ordered list; the reducer replaces its copy.
    WatchedFoldersChanged {
        folders: Vec<WatchedFolder>,
    },
    FolderCandidate {
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    },
    CandidateDiscovered {
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    },
    /// A leaf folder that looks like a release but failed validation
    /// (corrupt/zero-byte audio, corrupt image, CUE referencing missing audio).
    /// The reducer surfaces it under the Skipped tab with its reason. The key is
    /// the folder path, shared with `CandidateRemoved` for reconciliation.
    InvalidCandidate(InvalidCandidate),
    FolderReleaseBoundary(FolderReleaseBoundary),
    /// A candidate is gone: the watcher re-scanned its folder and the release
    /// no longer resolves on disk, or the folder it belonged to stopped being
    /// watched (one event per candidate the folder held). The reducer removes
    /// it by key (the key is the candidate's folder path); the extraction
    /// service cancels the key's in-flight extraction on this event.
    CandidateRemoved {
        candidate_key: String,
    },
    /// The user manually skipped or unskipped a candidate. The reducer flips the
    /// candidate's `skipped` flag in place; the import view re-tabs it from New
    /// to Skipped (or back). The key is the candidate's folder path.
    CandidateSkipChanged {
        candidate_key: String,
        skipped: bool,
    },
    /// The user bound one of a candidate's track sheets to an audio file, or
    /// cleared the binding. Carries the re-derived candidate, like
    /// [`Self::FolderCandidate`] — a bound sheet is a different disc, with a
    /// different track count and format label, so every index holding those
    /// replaces its copy from this rather than keeping stale ones.
    ///
    /// It also says the candidate's stored identify verdict was cleared, which
    /// is what brings it back to the queue sweep.
    CandidateBindingChanged {
        candidate: FolderCandidate,
    },
    CandidateVerdictStored {
        candidate_key: String,
    },
    /// The user chose an identity for the candidate — a pressing, or its own
    /// tags — and the choice was persisted. The triage projection re-reads so
    /// the row carries it.
    CandidateIdentityPicked {
        candidate_key: String,
    },
    FolderScanStatusChanged {
        status: WatchedFolderScanStatus,
    },
    Finished,
}

/// Commands to the folder-watch reconciliation task. The scan installs OS
/// watches from blocking work as directories are reached; synchronous callers
/// only persist intent and enqueue commands.
pub(crate) enum WatcherCommand {
    Rescan(std::path::PathBuf),
    Refresh {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    SetFolderReleaseDecision {
        target: (FolderReleaseDecisionKey, FolderReleaseDecision),
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Remove {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Shutdown {
        completion: std::sync::mpsc::Sender<()>,
    },
}

impl ImportServiceHandle {
    pub(crate) fn new(
        requests_tx: mpsc::UnboundedSender<crate::import::service::ImportWorkerMessage>,
        worker_thread: std::thread::JoinHandle<()>,
        watcher_thread: std::thread::JoinHandle<()>,
        library_manager: LibraryManager,
        clock: coven::ClockRef,
        ids: coven::IdRef,
        runtime_handle: tokio::runtime::Handle,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        candidate_state: Arc<Mutex<ImportCandidateState>>,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    ) -> Self {
        Self {
            requests_tx,
            worker_thread: Arc::new(Mutex::new(Some(worker_thread))),
            library_manager,
            clock,
            ids,
            event_tx,
            folder_registry,
            candidate_state,
            folder_state_commit,
            watcher_tx,
            watcher_thread: Arc::new(Mutex::new(Some(watcher_thread))),
            runtime_handle,
        }
    }

    pub(crate) fn start_candidate_services(
        &self,
    ) -> (
        crate::identify::IdentifyServiceHandle,
        crate::signals::ExtractionServiceHandle,
    ) {
        let identify = crate::identify::IdentifyServiceHandle::new(
            self.library_manager.clone(),
            self.runtime_handle.clone(),
            self.event_tx.clone(),
        );
        let extraction = crate::signals::ExtractionService::start(
            self.runtime_handle.clone(),
            self.event_tx.clone(),
            self.library_manager.clone(),
        );
        (identify, extraction)
    }

    /// Stop and join the worker thread. Idempotent (the join handle is taken
    /// once); called from `AppServicesInner`'s drop so the worker's
    /// `LibraryManager` clone — and the store-open lock it pins — is released
    /// before teardown returns. An explicit `Shutdown` message rather than
    /// channel closure: `self` holds a live sender, so the channel can't close
    /// before the join.
    pub fn stop_and_join(&self) {
        if let Some(watcher_thread) = self.watcher_thread.lock().unwrap().take() {
            let (completion, receiver) = std::sync::mpsc::channel();
            if self
                .watcher_tx
                .send(WatcherCommand::Shutdown { completion })
                .is_ok()
                && receiver.recv().is_err()
            {
                tracing::warn!("folder scan coordinator ended without acknowledging shutdown");
            }
            if let Err(panic) = watcher_thread.join() {
                tracing::warn!("folder scan coordinator panicked before join: {panic:?}");
            }
        }
        let Some(join_handle) = self.worker_thread.lock().unwrap().take() else {
            return;
        };
        if self
            .requests_tx
            .send(crate::import::service::ImportWorkerMessage::Shutdown)
            .is_err()
        {
            // The worker already exited (its loop only ends on Shutdown or a
            // panic); the join below surfaces which.
            tracing::warn!("import command channel closed before shutdown");
        }
        // A panicked worker thread already reported itself; joining from Drop
        // must not repropagate, but the panic shouldn't vanish either.
        if let Err(panic) = join_handle.join() {
            tracing::warn!("import worker thread panicked before join: {panic:?}");
        }
    }

    /// Broadcast a resumed identify state — a stored verdict standing back up
    /// as the state opening its candidate shows, with no run behind it. Rides
    /// the same event every live driver's transitions do, so the runtime
    /// recorder and both UIs consume it identically. The toolbar is empty
    /// because a resumed state has no live signals to badge (see
    /// [`crate::identify::TerminalVerdict::resume_state`]); `Interactive`
    /// because only a selection resumes one.
    pub(crate) fn broadcast_resumed_identify_state(
        &self,
        candidate_key: String,
        state: crate::identify::IdentifyState,
    ) {
        send_event(
            &self.event_tx,
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar: Vec::new(),
                priority: crate::util::rate_limiter::CallPriority::Interactive,
            },
        );
    }

    pub(crate) fn announce_candidate_verdict_stored(&self, candidate_key: String) {
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }),
        );
    }

    pub(crate) fn announce_queue_identify_progress(&self, identified: u32, total: u32) {
        send_event(
            &self.event_tx,
            ImportEvent::QueueIdentifyProgress { identified, total },
        );
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn emit_event_for_test(&self, event: ImportEvent) {
        send_event(&self.event_tx, event);
    }

    pub fn get_import_candidates(&self) -> ImportCandidatesSnapshot {
        let watched_folders = self.watched_folders();
        self.candidate_state
            .lock()
            .unwrap()
            .snapshot(watched_folders)
    }

    pub fn get_candidate(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        self.candidate_state.lock().unwrap().get(key)
    }

    /// Claim `candidate_key` for an import that is about to be queued.
    ///
    /// Takes the folder-state commit lock, which
    /// [`Self::save_candidate_verdict_if_current`] holds across *both* its
    /// check and its write. So by the time this returns, either that write has
    /// already landed or it has yet to read the candidate and will find it
    /// claimed — there is no interval in which a verdict is stored for a
    /// candidate whose import has been committed to.
    pub(crate) async fn claim_candidate_for_import(&self, candidate_key: &str) {
        let _commit = self.folder_state_commit.lock().await;
        self.candidate_state
            .lock()
            .unwrap()
            .claim_for_import(candidate_key);
    }

    async fn release_import_claim(&self, candidate_key: &str) {
        let _commit = self.folder_state_commit.lock().await;
        self.candidate_state
            .lock()
            .unwrap()
            .release_import_claim(candidate_key);
    }

    /// Store one candidate's verdict, unless the candidate has moved on from
    /// the shape the verdict describes — its files were re-decided, it was
    /// skipped, it is already in the library, or an import has claimed it.
    ///
    /// The commit lock spans the check and the write, and everything that can
    /// invalidate a verdict — a scan, a file re-decision, a skip, an import
    /// claim — is written under the same lock, so a `true` return means the row
    /// describes the candidate as it was at the moment it landed. (The UI event
    /// bus also records import progress into this state without the lock, but
    /// only ever onto a candidate an import already claimed.)
    pub(crate) async fn save_candidate_verdict_if_current(
        &self,
        candidate_key: &str,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, crate::library::LibraryError> {
        let _commit = self.folder_state_commit.lock().await;
        let current = matches!(
            self.candidate_state.lock().unwrap().get(candidate_key),
            Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime,
                actionable: true,
                skipped: false,
                is_added: false,
            }) if candidate.files.content_hash() == row.content_hash
                && candidate.file_edit_revision == row.expected_edit_revision
                && runtime.import_status.is_none()
        );
        if !current {
            return Ok(false);
        }
        self.library_manager
            .save_import_candidate_verdict(row)
            .await
    }
}

/// Remap the parsed (temporary) IDs a link row points at to their actual DB IDs.
///
/// A `ParsedAlbum`'s link rows reference artist and work IDs minted during
/// parsing; reconcile may have resolved those to existing DB rows. `label` names
/// the endpoint being remapped in the unmapped-ID error.
pub(crate) fn remap_links<T: Clone>(
    links: &[T],
    id_map: &HashMap<String, String>,
    label: &str,
    target_id: impl Fn(&T) -> &str,
    assign_target_id: impl Fn(&mut T, String),
) -> Result<Vec<T>, crate::import::ImportError> {
    links
        .iter()
        .map(|link| {
            let parsed_id = target_id(link);
            let actual_id =
                id_map
                    .get(parsed_id)
                    .ok_or_else(|| crate::import::ImportError::Internal {
                        detail: format!("{label} ID {parsed_id} not found in the import's ID map"),
                    })?;
            let mut remapped = link.clone();
            assign_target_id(&mut remapped, actual_id.clone());
            Ok(remapped)
        })
        .collect()
}

/// Project a parsed album (mapper output) into the editor's `ReleaseUserEdit`
/// shape. The one way the edit-metadata form is seeded, from every path: a
/// source-backed import (the prefetch's `seed`), an Unknown import's local
/// evidence, and reset-to-source's cached source payload.
///
/// It projects the very `ParsedAlbum` the commit worker applies the editor's
/// overlay onto, which is what lets `apply_user_edit_to_seed` tell an untouched
/// field from an edited one. Seeding the editor from any other shape — the
/// picker's release detail, say — makes an untouched artist list read as a
/// deletion and drops the release's secondary album artists.
///
/// Track artist names are emitted positionally per existing `track_artists` row;
/// an empty per-track list means "share the album artist" in the editor's
/// convention.
pub fn parsed_album_to_user_edit(parsed: &super::ParsedAlbum) -> crate::import::ReleaseUserEdit {
    // A ParsedAlbum is self-consistent by construction (the mapper builds its
    // artists and junctions together), so a missing reference is a bug here, not
    // a user-facing error.
    let album_artist_names = crate::import::artist_names::album_artist_names(
        &parsed.artists,
        &parsed.album_artists,
        &parsed.album.artist_id,
    )
    .expect("ParsedAlbum album_artists reference its own artists");

    let tracks = parsed
        .tracks
        .iter()
        .map(|t| {
            let artist_names = crate::import::artist_names::track_artist_names(
                &parsed.artists,
                &parsed.track_artists,
                &t.id,
            )
            .expect("ParsedAlbum track_artists reference its own artists");
            crate::import::TrackUserEdit {
                title: t.title.clone(),
                side: t.side,
                track_number: t.track_number,
                artist_names,
                // A seed says what the release is, not which of the folder's
                // audio backs each track; the track slots settle that, and
                // stamp the binding onto the rows they hand to the editor.
                file: None,
            }
        })
        .collect();

    crate::import::ReleaseUserEdit {
        album_title: parsed.album.title.clone(),
        album_artist_names,
        pressing: crate::import::PressingEdit {
            year: parsed.release.pressing.year,
            format: parsed.release.pressing.format.clone(),
            label: parsed.release.pressing.label.clone(),
            catalog_number: parsed.release.pressing.catalog_number.clone(),
            country: parsed.release.pressing.country.clone(),
            barcode: parsed.release.pressing.barcode.clone(),
        },
        tracks,
    }
}

/// Shape the prefetched editor seed for the user's identity claim:
///
/// - **Exact**: `pressing` stays as the picked release has it.
/// - **Approximate** / **Unknown**: `pressing` is `PressingEdit::blank()` — the
///   user didn't claim a specific pressing, so showing them the source's
///   pressing data would imply a claim they never made. They can still fill in
///   fields they know, and the overlay carries those edits to commit.
///
/// Everything else in the seed comes from the release itself (see
/// [`crate::import::search::ImportReleasePrefetch`]) and is the same under every
/// choice, so flipping the claim re-runs this over the kept seed instead of
/// re-fetching.
///
/// The UI calls this rather than branching on `IdentityChoice` itself — the
/// bridge stays thin.
pub fn shape_user_edit_for_choice(
    seed: &super::ReleaseUserEdit,
    choice: &super::IdentityChoice,
) -> super::ReleaseUserEdit {
    let mut edit = seed.clone();
    match choice {
        super::IdentityChoice::Exact { .. } => {}
        super::IdentityChoice::Approximate { .. } | super::IdentityChoice::Unknown => {
            edit.pressing = super::PressingEdit::blank();
        }
    }
    edit
}

/// The files the import pipeline writes rows for and accounts bytes against, in
/// the release's own `relative_path` order.
///
/// Reads [`CategorizedFiles::release_files`], the same iterator
/// [`CategorizedFiles::content_hash`] covers — the payload and the fingerprint
/// that identifies it are one set by construction.
pub(crate) fn flatten_categorized_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<crate::import::folder_scanner::ScannedFile> {
    categorized.release_files().cloned().collect()
}
