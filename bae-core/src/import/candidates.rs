//! What one import candidate is beyond the rows the scan wrote, and which
//! stored entries a new one replaces.
//!
//! Everything a row shows that outlives the process — the scanned folder and
//! its files, whether it was skipped, whether its content is already in the
//! library, which boundary decisions exposed it, the scan status of its root,
//! the identify state its stored verdict stands back up as, the signals
//! extraction settled on, the release an import wrote and the error one failed
//! with — is a fact in a table, read by [`crate::import::list`]. What is left
//! is what is happening *right now*: a run in flight and an import in
//! progress. That is [`CandidateRuntimeSnapshot`], held by
//! [`super::candidate_runtime::CandidateRuntime`] and delivered per key.

use super::folder_scanner::{
    CategorizedFiles, FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate, ScanItem,
};
use super::types::ImportStep;
use std::collections::HashSet;
use std::path::Path;

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

/// One candidate by key, with its runtime joined: the read behind every
/// "what is this key right now" question.
#[derive(Debug, Clone)]
pub enum ImportCandidateSnapshot {
    Folder {
        candidate: FolderCandidate,
        /// `None` when nothing is running for this key.
        runtime: Option<CandidateRuntimeSnapshot>,
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

/// What is in flight for one key. An entry exists only while at least one
/// field is `Some`; both `None` is the absence of an entry, not a value.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRuntimeSnapshot {
    /// The live driver's current state, never
    /// [`IdentifyState::Idle`](crate::identify::IdentifyState::Idle). `None`
    /// when no driver has reported and nothing terminal is being held.
    pub identify: Option<crate::identify::IdentifyState>,
    /// The running import: claimed, preparing, or partway through a phase.
    pub import: Option<ImportInFlight>,
}

/// How far a running import has got. It ends with the import: a finished one
/// leaves the runtime and reads back off the release row it wrote, or the
/// failure row it wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInFlight {
    pub progress_percent: u32,
    pub step: Option<ImportStep>,
}

/// The library release one candidate's bytes were imported as.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRelease {
    pub release_id: String,
    pub album_id: String,
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
