//! The import tab's candidate list, read from the durable folder-scan rows.
//!
//! Everything a row shows that outlives the process — the scanned folder and
//! its files, whether it was skipped, whether its content is already in the
//! library, which boundary decisions exposed it, the scan status of its root,
//! the identify state its stored verdict stands back up as — is a fact in a
//! table, and [`ImportCandidatesSnapshot`] is one read of those tables. What a
//! row shows that does not outlive the process (a run in flight, extracted
//! signals, an import's progress) is [`CandidateRuntimeSnapshot`], held by
//! [`super::candidate_runtime::CandidateRuntime`] and delivered per key.

use super::folder_registry::WatchedFolder;
use super::folder_scanner::{
    CategorizedFiles, FolderCandidate, FolderReleaseBoundary, FolderReleaseDecision,
    FolderReleaseDecisionKey, InvalidCandidate, ScanItem,
};
use super::types::ImportStep;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[cfg(test)]
mod tests;

/// The durable candidate list: one value per read of the folder-scan tables.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidatesSnapshot {
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_candidates: Vec<FolderImportCandidateSnapshot>,
    pub invalid_candidates: Vec<InvalidCandidate>,
    pub boundaries: Vec<FolderReleaseBoundary>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
}

impl ImportCandidatesSnapshot {
    pub fn folder_candidate(&self, key: &str) -> Option<&FolderImportCandidateSnapshot> {
        self.folder_candidates
            .iter()
            .find(|candidate| candidate.candidate.path.to_string_lossy() == key)
    }
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct FolderImportCandidateSnapshot {
    pub candidate: FolderCandidate,
    /// `false` for a release approximation found before its enclosing folder
    /// boundary was known: visible scan progress that cannot be identified or
    /// imported until a later scan item settles it.
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
    /// The identify state the candidate's stored verdict stands back up as,
    /// with the live library statuses of the releases it names — the answer a
    /// row shows when no run is in flight. `Idle` when nothing is stored for
    /// the candidate's current file shape.
    pub resumed_identify_state: crate::identify::IdentifyState,
}

/// One candidate by key, with its runtime joined: the read behind every
/// "what is this key right now" question.
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
    /// A key with runtime but no scanned folder — a library release being
    /// re-identified.
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRuntimeSnapshot {
    pub identify_state: crate::identify::IdentifyState,
    pub toolbar: Vec<crate::identify::ToolbarSignal>,
    pub signals: Option<crate::signals::Signals>,
    pub import_status: Option<CandidateImportStatusSnapshot>,
}

impl CandidateRuntimeSnapshot {
    pub fn idle() -> Self {
        Self {
            identify_state: crate::identify::IdentifyState::Idle,
            toolbar: Vec::new(),
            signals: None,
            import_status: None,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.identify_state, crate::identify::IdentifyState::Idle)
            && self.toolbar.is_empty()
            && self.signals.is_none()
            && self.import_status.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRelease {
    pub release_id: String,
    pub album_id: String,
}

#[derive(Debug, Clone, PartialEq)]
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

/// Whether a candidate at `candidate_path` under `watched_folder_path` is in
/// the stored skip set, which keys by root-relative path.
pub(crate) fn is_skipped(
    skipped: &HashSet<(String, String)>,
    watched_folder_path: &str,
    candidate_path: &Path,
) -> Result<bool, super::ImportError> {
    let relative =
        super::folder_registry::candidate_relative_path(watched_folder_path, candidate_path)?;
    Ok(skipped.contains(&(watched_folder_path.to_string(), relative)))
}

/// Assemble the list from one read of the scan tables, in the order the tab
/// shows it: watched folders in their stored order, candidates in natural
/// path order within each.
///
/// `resumed_identify_state` is `Idle` on every row here; the database
/// projection fills it in from the stored verdicts it reads in the same
/// snapshot.
pub(crate) fn build_snapshot(
    watched_folders: Vec<WatchedFolder>,
    scans: Vec<crate::db::DbFolderScanSnapshot>,
    skipped: &HashSet<(String, String)>,
    imported_content_hashes: &HashSet<String>,
) -> Result<ImportCandidatesSnapshot, super::ImportError> {
    let watched_order: HashMap<&str, usize> = watched_folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.path.as_str(), index))
        .collect();
    let order_for = |path: &str| match watched_order.get(path) {
        Some(index) => *index,
        None => usize::MAX,
    };

    let mut folder_candidates = Vec::new();
    let mut invalid_candidates = Vec::new();
    let mut boundaries = Vec::new();
    let mut folder_scan_statuses = Vec::with_capacity(scans.len());
    for scan in scans {
        let watched_folder = watched_folders
            .iter()
            .find(|folder| folder.path == scan.watched_folder_path)
            .ok_or_else(|| super::ImportError::Internal {
                detail: format!(
                    "folder scan root {} is not a watched folder",
                    scan.watched_folder_path
                ),
            })?;
        folder_scan_statuses.push(WatchedFolderScanStatus {
            watched_folder_path: scan.watched_folder_path.clone(),
            watched_folder_name: watched_folder.name.clone(),
            status: scan.status,
        });
        for item in scan.items {
            match item {
                ScanItem::Discovered(candidate) => folder_candidates.push(folder_row(
                    candidate,
                    false,
                    skipped,
                    imported_content_hashes,
                )?),
                ScanItem::Valid(candidate) => folder_candidates.push(folder_row(
                    candidate,
                    true,
                    skipped,
                    imported_content_hashes,
                )?),
                ScanItem::Invalid(candidate) => invalid_candidates.push(candidate),
                ScanItem::Boundary(boundary) => boundaries.push(boundary),
            }
        }
    }
    folder_candidates.sort_by(|left, right| {
        order_for(&left.candidate.watched_folder_path)
            .cmp(&order_for(&right.candidate.watched_folder_path))
            .then_with(|| {
                natord::compare_ignore_case(
                    &left.candidate.display_path,
                    &right.candidate.display_path,
                )
            })
    });
    invalid_candidates.sort_by(|left, right| {
        order_for(&left.watched_folder_path)
            .cmp(&order_for(&right.watched_folder_path))
            .then_with(|| natord::compare_ignore_case(&left.display_path, &right.display_path))
    });
    boundaries.sort_by(|left, right| {
        order_for(&left.key.watched_folder_path)
            .cmp(&order_for(&right.key.watched_folder_path))
            .then_with(|| {
                left.key
                    .relative_folder_path
                    .cmp(&right.key.relative_folder_path)
            })
    });
    folder_scan_statuses.sort_by(|left, right| {
        order_for(&left.watched_folder_path)
            .cmp(&order_for(&right.watched_folder_path))
            .then_with(|| left.watched_folder_path.cmp(&right.watched_folder_path))
    });
    Ok(ImportCandidatesSnapshot {
        watched_folders,
        folder_candidates,
        invalid_candidates,
        boundaries,
        folder_scan_statuses,
    })
}

fn folder_row(
    candidate: FolderCandidate,
    actionable: bool,
    skipped: &HashSet<(String, String)>,
    imported_content_hashes: &HashSet<String>,
) -> Result<FolderImportCandidateSnapshot, super::ImportError> {
    let is_skipped = is_skipped(skipped, &candidate.watched_folder_path, &candidate.path)?;
    let is_added = imported_content_hashes.contains(&candidate.files.content_hash());
    Ok(FolderImportCandidateSnapshot {
        candidate,
        actionable,
        skipped: is_skipped,
        is_added,
        resumed_identify_state: crate::identify::IdentifyState::Idle,
    })
}

/// One stored entry under a root, as much of it as supersession needs: its
/// key, and whether it is a boundary rather than a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredEntryKey {
    pub key: String,
    pub is_boundary: bool,
}

/// The stored entries under `item`'s root that `item` replaces when it is
/// written, so the write deletes them in the same transaction.
///
/// A valid candidate exposed by resolved boundary decisions replaces what
/// those decisions hid: everything below a combined folder, or exactly the
/// row at a kept-separate folder — including the boundary entry itself, whose
/// key is that folder's path. An invalid candidate replaces only the boundary
/// entries its decisions resolved. A boundary replaces the tentative candidates
/// it hides. A tentative (discovered) candidate replaces nothing.
pub(crate) fn superseded_entry_keys(existing: &[StoredEntryKey], item: &ScanItem) -> Vec<String> {
    let own_key = item.persisted_key();
    let mut removed = match item {
        ScanItem::Discovered(_) => Vec::new(),
        ScanItem::Valid(candidate) => {
            let decisions: Vec<(FolderReleaseDecisionKey, FolderReleaseDecision)> = candidate
                .resolved_boundaries
                .iter()
                .map(|resolved| (resolved.key.clone(), resolved.decision))
                .collect();
            let keys: HashSet<String> = existing
                .iter()
                .filter(|entry| entry.key != own_key)
                .map(|entry| entry.key.clone())
                .collect();
            super::folder_scanner::release_decision_removed_keys(&keys, &decisions)
        }
        ScanItem::Invalid(candidate) => {
            let boundary_keys: HashSet<String> = candidate
                .resolved_boundaries
                .iter()
                .map(|resolved| {
                    Path::new(&resolved.key.watched_folder_path)
                        .join(&resolved.key.relative_folder_path)
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            existing
                .iter()
                .filter(|entry| entry.is_boundary && boundary_keys.contains(&entry.key))
                .map(|entry| entry.key.clone())
                .collect()
        }
        ScanItem::Boundary(boundary) => boundary.candidate_keys.clone(),
    };
    removed.retain(|key| key != &own_key);
    removed.sort();
    removed.dedup();
    removed
}

/// The enclosing unresolved boundaries that become separate when `key` is
/// decided, or `None` when `key` is not a boundary the stored scan of its
/// root currently exposes.
///
/// `Some(empty)` covers a key that is already a boundary or a resolved one on
/// a row, and a first-level folder that groups rows today: deciding it needs
/// no ancestors. Deeper keys are looked up in the boundary tree rows, which
/// carry their ancestors.
pub(crate) fn release_boundary_ancestor_keys(
    items: &[ScanItem],
    key: &FolderReleaseDecisionKey,
) -> Option<Vec<FolderReleaseDecisionKey>> {
    let boundaries = items.iter().filter_map(|item| match item {
        ScanItem::Boundary(boundary) => Some(boundary),
        _ => None,
    });
    let resolved_on_row = items.iter().any(|item| match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate
            .resolved_boundaries
            .iter()
            .any(|resolved| resolved.key == *key),
        ScanItem::Invalid(candidate) => candidate
            .resolved_boundaries
            .iter()
            .any(|resolved| resolved.key == *key),
        ScanItem::Boundary(_) => false,
    });
    if resolved_on_row || boundaries.clone().any(|boundary| boundary.key == *key) {
        return Some(Vec::new());
    }
    let first_component_matches = |watched_folder_path: &str, display_path: &str| {
        watched_folder_path == key.watched_folder_path
            && display_path
                .split('/')
                .next()
                .is_some_and(|first| first == key.relative_folder_path)
    };
    let grouped_candidates = items
        .iter()
        .filter(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                first_component_matches(&candidate.watched_folder_path, &candidate.display_path)
            }
            ScanItem::Invalid(candidate) => {
                first_component_matches(&candidate.watched_folder_path, &candidate.display_path)
            }
            ScanItem::Boundary(_) => false,
        })
        .count();
    let grouped_boundaries = boundaries.clone().any(|boundary| {
        first_component_matches(&boundary.key.watched_folder_path, &boundary.display_path)
    });
    if !key.relative_folder_path.contains('/') && (grouped_candidates >= 1 || grouped_boundaries) {
        return Some(Vec::new());
    }
    let row = boundaries
        .flat_map(|boundary| &boundary.tree_rows)
        .find(|row| row.decision_key == *key)?;
    Some(row.ancestor_decision_keys.clone())
}

/// Every stored folder candidate that shares one file-decision identity, with
/// its files — the set a file decision settles together.
pub(crate) fn files_for_identity(
    items: &[ScanItem],
    content_hash: &str,
    edit_revision: u64,
) -> Vec<(String, CategorizedFiles)> {
    items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)
                if candidate.files.content_hash() == content_hash
                    && candidate.file_edit_revision == edit_revision =>
            {
                Some((
                    candidate.path.to_string_lossy().into_owned(),
                    candidate.files.clone(),
                ))
            }
            _ => None,
        })
        .collect()
}
