//! What one import candidate is beyond the rows the scan wrote, and which
//! stored entries a new one replaces.
//!
//! Everything a row shows that outlives the process — the scanned folder and
//! its files, whether it was skipped, whether this device imported its files,
//! which boundary decisions exposed it, the scan status of its root, the
//! identify state its stored verdict stands back up as, the signals extraction
//! settled on, the release an import wrote and the error one failed with — is
//! a fact in a table, read by [`crate::import::list`]. What is left
//! is what is happening *right now*: a run in flight and an import in
//! progress. Where each stands is the standings table's, which the list reads
//! beside the rows; [`CandidateRuntimeSnapshot`] is that standing with what
//! [`super::candidate_runtime::CandidateRuntime`] holds beside it — the state
//! a run published, how far an import has got — delivered per key.

use super::folder_scanner::{FolderCandidate, InvalidCandidate};
use super::types::ImportStep;
use crate::identify::IdentifyState;

/// Where a stored candidate stands in the queue, read once from the three
/// places that each hold one fact about it: the skip table, the releases this
/// device imported, and the standings table's import claims. Every gate that decides what
/// may be done to a candidate reads this rather than composing the three
/// on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateStanding {
    /// The person set it aside.
    pub skipped: bool,
    /// This device imported a release from its files.
    pub imported: bool,
    /// An import has claimed it and is running.
    pub claimed: bool,
}

impl CandidateStanding {
    /// Whether identification may still answer for it: not set aside, not
    /// imported here, not being imported.
    pub fn answerable(&self) -> bool {
        !self.skipped && !self.imported && !self.claimed
    }

    /// Whether its preparation may still be changed. A skipped candidate may
    /// be: skipping sets it aside, it does not freeze it.
    pub fn editable(&self) -> Result<(), super::ImportError> {
        if self.claimed {
            return Err(super::ImportError::CandidateImportInProgress);
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

/// Why a run's answer did not land, which decides whether identifying the
/// candidate again can land it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveFailure {
    /// The answer was not written; another run may write it.
    NotWritten { error: String },
    /// The release the answer picks could not be read into the folder's
    /// draft. Another run reaches the same answer and fails the same way.
    Inapplicable { error: String },
}

impl SaveFailure {
    pub fn error(&self) -> &str {
        match self {
            Self::NotWritten { error } | Self::Inapplicable { error } => error,
        }
    }

    /// Whether identifying the candidate again can land what this run could
    /// not.
    pub fn retryable(&self) -> bool {
        match self {
            Self::NotWritten { .. } => true,
            Self::Inapplicable { .. } => false,
        }
    }
}

/// What is in flight for one key. An entry exists only while one of its
/// parts is `Some`; all of them `None` is the absence of an entry, not a
/// value.
///
/// Where a folder candidate's identification and import stand is read from
/// the standings table, the only place it is held; the state a run published
/// and how far an import has got come from the candidate runtime beside it,
/// and so does the whole of a library release's re-identify run.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRuntimeSnapshot {
    pub identification: Option<IdentificationRuntime>,
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

impl CandidateRuntimeSnapshot {
    /// What identification is doing for the key, as its pane shows it: the
    /// run in flight or the answer being written, else the wait for a run.
    /// `None` when nothing is identifying the key, and its pane shows the
    /// stored verdict; a write that failed reaches the pane through the row.
    pub fn identification_in_flight(&self) -> Option<IdentificationInFlight> {
        match self.identification.as_ref()? {
            IdentificationRuntime::Queued(_) => Some(IdentificationInFlight::Queued),
            IdentificationRuntime::Running(state) => {
                Some(IdentificationInFlight::Run(state.clone()))
            }
            IdentificationRuntime::Saving(answer) => {
                Some(IdentificationInFlight::Run(answer.state.clone()))
            }
            IdentificationRuntime::SaveFailed(_) => None,
        }
    }
}

/// Where one key's identification stands right now.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentificationRuntime {
    /// Planned for this key and not started.
    Queued(Admission),
    /// The latest state the run in flight published — its answer, once it
    /// reaches one, until the standings table says the answer is being
    /// written.
    Running(IdentifyState),
    /// The answer a run reached, while it is being written.
    Saving(SavingAnswer),
    /// Why the last write of an answer did not land. Cleared by the next run
    /// of this key.
    SaveFailed(SaveFailure),
}

/// A run's answer while it is being written, and what was asked of it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavingAnswer {
    /// Always terminal.
    pub state: IdentifyState,
    /// A person asked, while the release this answer picks unattended was
    /// being read, for the candidate to be imported once the answer stores
    /// it. Only an answer that picks unattended carries the ask, and it goes
    /// with the answer.
    pub import_asked: bool,
}

/// What identification is doing for a key right now.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentificationInFlight {
    /// On the identification queue, its run not started.
    Queued,
    /// The latest state a run published: in flight, or the answer being
    /// written. Never `Idle`.
    Run(IdentifyState),
}

/// The files an identification is of: a candidate's content hash and its
/// file-decision revision. Candidates sharing them are one identification
/// job, since the answer is stored under them, and a standing recorded for
/// other files than a candidate has now is not that candidate's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CandidateFiles {
    pub content_hash: String,
    pub file_edit_revision: u64,
}

impl CandidateFiles {
    pub fn of(candidate: &FolderCandidate) -> Self {
        Self {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: candidate.file_edit_revision,
        }
    }
}

/// How a candidate got on the identification queue: found while
/// identification ran on its own, or asked for by a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Automatic,
    Requested,
}

/// The step a running import is at. It ends with the import: a finished one
/// leaves the runtime and reads back off the release row it wrote, or the
/// failure row it wrote. How far through the step it is moves many times a
/// second, so it is the key's percent in
/// [`CandidateRuntime::watch_import_percents`](crate::import::CandidateRuntime::watch_import_percents).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInFlight {
    pub step: ImportStep,
}

/// The release this device imported from one candidate's files, found by the
/// folder hash it recorded (`local_release_origin`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRelease {
    pub release_id: String,
    pub album_id: String,
}
