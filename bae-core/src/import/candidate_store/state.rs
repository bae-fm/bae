use super::*;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct ImportCandidatesSnapshot {
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_candidates: Vec<FolderImportCandidateSnapshot>,
    pub runtime_candidates: Vec<RuntimeImportCandidateSnapshot>,
    pub invalid_candidates: Vec<InvalidCandidate>,
    pub boundaries: Vec<FolderReleaseBoundary>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
}

#[derive(Debug, Clone)]
pub struct RuntimeImportCandidateSnapshot {
    pub key: String,
    pub runtime: CandidateRuntimeSnapshot,
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
pub struct ImportedRelease {
    pub release_id: String,
    pub album_id: String,
}

#[derive(Debug, Clone)]
pub enum CandidateImportStatusSnapshot {
    Importing {
        progress_percent: u32,
        step: Option<ImportStep>,
    },
    Complete {
        release: ImportedRelease,
    },
    CloudUploadQueued {
        release: ImportedRelease,
        outbox_revision: u64,
    },
    Error {
        error: String,
    },
}

#[derive(Debug, Clone, Default)]
pub(super) struct CandidateState {
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

impl CandidateState {
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
                step: Some(ImportStep::Preparing(PrepareStep::Queued)),
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
                    ImportProgress::Progress { percent, phase, .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: *percent as u32,
                            step: Some(ImportStep::Running(*phase)),
                        })
                    }
                    ImportProgress::Complete { id, album_id, .. } => {
                        Some(CandidateImportStatusSnapshot::Complete {
                            release: ImportedRelease {
                                release_id: id.clone(),
                                album_id: album_id.clone(),
                            },
                        })
                    }
                    ImportProgress::RemoteUploadQueued {
                        id,
                        album_id,
                        outbox_revision,
                        ..
                    } => Some(CandidateImportStatusSnapshot::CloudUploadQueued {
                        release: ImportedRelease {
                            release_id: id.clone(),
                            album_id: album_id.clone(),
                        },
                        outbox_revision: *outbox_revision,
                    }),
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
                //
                // What the retained terminal state covers is bounded: the
                // interval before the verdict's durable write lands (cleared
                // by `CandidateVerdictStored` below), and terminal states
                // that never store — a settle shaped by a lookup that never
                // answered — which are session-only by design.
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
            // The candidate's answer now lives in its stored verdict row, and
            // the candidates subscription serves it from there. The recorded
            // terminal state has done its job — carrying the answer across the
            // interval between settling and the durable write — so it clears,
            // leaving the runtime to hold only what has no row: runs in
            // flight, and extraction's signals, which are facts about the
            // files rather than about this run. Only a terminal state clears:
            // a newer run's in-flight state must not be blanked by the
            // previous run's write landing.
            ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }) => {
                if let Some(runtime) = self.runtime.get_mut(candidate_key) {
                    if runtime.identify_state.is_terminal() {
                        runtime.identify_state = crate::identify::IdentifyState::Idle;
                        runtime.toolbar = Vec::new();
                    }
                }
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
        let mut runtime_candidates: Vec<_> = self
            .runtime
            .iter()
            .filter(|(key, _)| key.starts_with(Self::REIDENTIFY_PREFIX))
            .map(|(key, runtime)| RuntimeImportCandidateSnapshot {
                key: key.clone(),
                runtime: runtime.clone(),
            })
            .collect();
        runtime_candidates.sort_by(|left, right| left.key.cmp(&right.key));

        ImportCandidatesSnapshot {
            watched_folders,
            folder_candidates: folder_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
            runtime_candidates,
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
