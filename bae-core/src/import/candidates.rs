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
use crate::identify::IdentifyState;

/// Where a stored candidate stands in the queue, read once from the three
/// places that each hold one fact about it: the skip table, the library's
/// releases, and the runtime's import claims. Every gate that decides what
/// may be done to a candidate reads this rather than composing the three
/// on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateStanding {
    /// The person set it aside.
    pub skipped: bool,
    /// Its files are already a release in the library.
    pub imported: bool,
    /// An import has claimed it and is running.
    pub claimed: bool,
}

impl CandidateStanding {
    /// Whether identification may still answer for it: not set aside, not
    /// already in the library, not being imported.
    pub fn answerable(&self) -> bool {
        !self.skipped && !self.imported && !self.claimed
    }

    /// Whether its preparation may still be changed. A skipped candidate may
    /// be: skipping sets it aside, it does not freeze it.
    pub fn editable(&self) -> Result<(), super::ImportError> {
        if self.claimed {
            return Err(super::ImportError::CandidateImportInProgress);
        }
        if self.imported {
            return Err(super::ImportError::CandidateAlreadyImported);
        }
        Ok(())
    }
}

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

/// What is in flight for one key, one fact per field. An entry exists only
/// while at least one of them is `Some`; all of them `None` is the absence of
/// an entry, not a value. No field is inferred from another or from the order
/// events arrived in — each has exactly one writer.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRuntimeSnapshot {
    /// Identification is planned for this key and has not started. `None` once
    /// its run starts, and for a key nobody queued.
    pub queued: Option<Admission>,
    /// The latest state a run in flight published. Never terminal — a run's
    /// terminal state is its answer, and answering ends it.
    pub running: Option<IdentifyState>,
    /// The answer a run reached, held until whoever asked for it says what
    /// became of it. Always terminal.
    pub saving: Option<IdentifyState>,
    /// Why the last write of a terminal state did not land. Cleared by the
    /// next run of this key.
    pub save_failed: Option<String>,
    /// The running import: claimed, preparing, or partway through a phase.
    pub import: Option<ImportInFlight>,
    /// The typed search a person submitted for this candidate, as its sources
    /// land. `None` before one is submitted and after it is cleared.
    ///
    /// It lives here rather than in the pane because its sources land one at a
    /// time and the pane is rebuilt from stored state: a value the pane owned
    /// would be lost on every redraw, and lost outright if the person looked
    /// at another candidate while a provider was still answering. The runtime
    /// holds the one each landing folds into and derives this from it, so what
    /// a surface draws cannot disagree with what a landing reads back.
    pub search: Option<super::candidate_search::CandidateSearch>,
}

/// How a candidate was admitted to identification: by the automatic policy
/// that answers the whole queue, or because a person asked for this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Automatic,
    Requested,
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

/// Whether reading the folder at `key` as `decision` is something the stored
/// scan offers — what a header's or a row's control points at.
///
/// A control acting on a folder this scan no longer reads that way is stale,
/// and writing its decision would settle a folder that is not there. A folder
/// is offered combined when two releases or more are stored below it, and
/// offered separate when the release stored for it is its grouping.
pub(crate) fn offers_folder_reading(
    items: &[ScanItem],
    key: &FolderReleaseDecisionKey,
    decision: FolderReleaseDecision,
) -> bool {
    let folder = std::path::Path::new(&key.watched_folder_path).join(&key.relative_folder_path);
    let releases = items.iter().filter_map(|item| match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            Some((&candidate.path, candidate.grouping.is_some()))
        }
        ScanItem::Invalid(candidate) => Some((&candidate.path, candidate.grouping.is_some())),
        ScanItem::Decided { .. } | ScanItem::Sidecar(_) => None,
    });
    match decision {
        FolderReleaseDecision::CombineAsOneRelease => {
            releases.filter(|(path, _)| path.starts_with(&folder)).count() >= 2
        }
        FolderReleaseDecision::KeepAsSeparateReleases => {
            releases.into_iter().any(|(path, grouped)| grouped && *path == folder)
        }
    }
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
                Some((candidate.key(), candidate.files.clone()))
            }
            _ => None,
        })
        .collect()
}
