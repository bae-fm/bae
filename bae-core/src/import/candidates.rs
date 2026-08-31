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

#[derive(Debug, Clone, PartialEq)]
pub struct WatchedFolderScanStatus {
    pub watched_folder_path: String,
    pub watched_folder_name: String,
    pub status: FolderScanStatus,
    /// Whether this folder lives on a volume served over the network, which
    /// changes how it is watched: a filesystem watch on such a volume reports
    /// only what this machine does to it, so the folder is checked on a
    /// schedule as well. The list says so, because "I added an album on the
    /// server and bae has not noticed" is otherwise a mystery.
    pub on_network_volume: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderScanStatus {
    Scanning { found_count: u64 },
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
    /// Identification admitted to the queue or reported by its driver. `None`
    /// when no identification work exists for this key.
    pub identify: Option<CandidateIdentifyRuntime>,
    /// The running import: claimed, preparing, or partway through a phase.
    pub import: Option<ImportInFlight>,
}

/// What currently exists for one candidate's identification.
///
/// Construction stays inside the import runtime so a reported
/// [`IdentifyState::Idle`](crate::identify::IdentifyState::Idle) cannot be
/// represented: idle means this value is absent.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateIdentifyRuntime(CandidateIdentifyRuntimeKind);

#[derive(Debug, Clone, PartialEq)]
enum CandidateIdentifyRuntimeKind {
    Queued(IdentifyQueueOwner),
    Reported(crate::identify::IdentifyState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentifyQueueOwner {
    AutomaticSweep,
    ExplicitLookup,
}

impl CandidateIdentifyRuntime {
    pub(crate) fn automatic_queue() -> Self {
        Self(CandidateIdentifyRuntimeKind::Queued(
            IdentifyQueueOwner::AutomaticSweep,
        ))
    }

    pub(crate) fn explicit_queue() -> Self {
        Self(CandidateIdentifyRuntimeKind::Queued(
            IdentifyQueueOwner::ExplicitLookup,
        ))
    }

    /// Wrap a driver's non-idle state. Idle is represented by this value being
    /// absent from [`CandidateRuntimeSnapshot`].
    pub fn from_state(state: crate::identify::IdentifyState) -> Option<Self> {
        (!matches!(state, crate::identify::IdentifyState::Idle))
            .then_some(Self(CandidateIdentifyRuntimeKind::Reported(state)))
    }

    pub(crate) fn is_automatic_queue(&self) -> bool {
        matches!(
            self.0,
            CandidateIdentifyRuntimeKind::Queued(IdentifyQueueOwner::AutomaticSweep)
        )
    }

    pub(crate) fn is_terminal(&self) -> bool {
        match &self.0 {
            CandidateIdentifyRuntimeKind::Queued(_) => false,
            CandidateIdentifyRuntimeKind::Reported(state) => state.is_terminal(),
        }
    }

    /// The state a driver reported, or `None` while the work is queued.
    pub fn state(&self) -> Option<&crate::identify::IdentifyState> {
        match &self.0 {
            CandidateIdentifyRuntimeKind::Queued(_) => None,
            CandidateIdentifyRuntimeKind::Reported(state) => Some(state),
        }
    }

    /// The state a driver reported, or `None` while the work is queued.
    pub fn into_state(self) -> Option<crate::identify::IdentifyState> {
        match self.0 {
            CandidateIdentifyRuntimeKind::Queued(_) => None,
            CandidateIdentifyRuntimeKind::Reported(state) => Some(state),
        }
    }
}

/// How far a running import has got. It ends with the import: a finished one
/// leaves the runtime and reads back off the release row it wrote, or the
/// failure row it wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInFlight {
    pub progress_percent: Option<u32>,
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
    /// Whether the entry at this key is the folder itself rather than
    /// something inside it: the candidate that reads the whole folder as one
    /// release. A folder that holds tracks of its own has a candidate under the
    /// same key which is *not* this — it is one of the releases inside it.
    pub covers_whole_folder: bool,
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
    let own_key = item.persisted_key().unwrap_or_default();
    let mut removed = match item {
        ScanItem::Discovered(_) => Vec::new(),
        ScanItem::Valid(candidate) => {
            let decisions: Vec<(FolderReleaseDecisionKey, FolderReleaseDecision)> = candidate
                .resolved_boundaries
                .iter()
                .map(|resolved| (resolved.key.clone(), resolved.decision))
                .collect();
            let others: Vec<StoredEntryKey> = existing
                .iter()
                .filter(|entry| entry.key != own_key)
                .cloned()
                .collect();
            super::folder_scanner::release_decision_removed_keys(&others, &decisions)
        }
        // A folder that failed validation replaces nothing: what stood at its
        // key is its own prior row, which the write deletes anyway.
        ScanItem::Invalid(_) => Vec::new(),
        // Not a scan entry: it stores as the folder's decision.
        ScanItem::Decided { .. } => Vec::new(),
    };
    removed.retain(|key| key != &own_key);
    removed.sort();
    removed.dedup();
    removed
}

/// Whether `key` names a folder the stored scan currently offers a reading
/// for — the one a group header's control or a combined row's menu points at.
///
/// A key that names nothing is a stale control acting on a folder this scan no
/// longer reads that way, and writing its decision would settle a folder that
/// is not there. Three things make a key current: a row settled by it, a row
/// that names it as the folder its releases could be read as one, and a first
/// path component with rows under it, which is what a group header stands for.
pub(crate) fn names_a_current_folder_reading(
    items: &[ScanItem],
    key: &FolderReleaseDecisionKey,
) -> bool {
    let resolved_on_row = items.iter().any(|item| match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate
                .resolved_boundaries
                .iter()
                .any(|resolved| resolved.key == *key)
                || candidate.combine_ancestor_key.as_ref() == Some(key)
        }
        ScanItem::Invalid(candidate) => candidate
            .resolved_boundaries
            .iter()
            .any(|resolved| resolved.key == *key),
        ScanItem::Decided { .. } => false,
    });
    if resolved_on_row {
        return true;
    }
    let first_component_matches = |watched_folder_path: &str, display_path: &str| {
        watched_folder_path == key.watched_folder_path
            && display_path
                .split('/')
                .next()
                .is_some_and(|first| first == key.relative_folder_path)
    };
    !key.relative_folder_path.contains('/')
        && items.iter().any(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                first_component_matches(&candidate.watched_folder_path, &candidate.display_path)
            }
            ScanItem::Invalid(candidate) => {
                first_component_matches(&candidate.watched_folder_path, &candidate.display_path)
            }
            ScanItem::Decided { .. } => false,
        })
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
