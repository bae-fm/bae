//! What the import queue derives about one candidate, keyed by the content
//! hash of its files: what identification concluded, and what a person
//! settled through its pane.

use super::*;

/// What a caller supplies to record one candidate's identify verdict via
/// [`crate::db::Database::save_import_candidate_verdict`] — the identify
/// columns of `import_candidate_state` except `identified_at`. That column is
/// stamped by the write path from the injected clock, the same convention as
/// `created_at` in `db/client/identity.rs`/`release.rs`: a timestamp that
/// records "when this write happened" is the DB layer's to assign, not data a
/// caller hands in — carrying it here would let a caller lie about it, and
/// would mean the sweep reaching for the ambient wall clock instead of the
/// fake-able one already threaded through `Database`.
///
/// It carries no file decisions: those are the user's half of the row and the
/// verdict write leaves them alone.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub struct NewImportCandidateVerdict {
    /// `CategorizedFiles::content_hash` — the row's identity. Adding,
    /// removing, or resizing a file changes this, which orphans the old row
    /// rather than updating it.
    pub content_hash: String,
    /// Where the candidate was last seen on disk. Not identity — the hash is —
    /// so a moved folder keeps reading the same row under its unchanged hash.
    pub folder_path: String,
    pub verdict: crate::identify::TerminalVerdict,
    /// The settled signals the run reached this verdict on: the disc ID, the
    /// barcodes, the classified text, and what every audio unit plays for.
    /// Stored beside the verdict so the pane and the queue read them back
    /// instead of extracting them again, and the `probed_total_duration_ms`
    /// column is summed from the durations by the write itself.
    pub signals: crate::signals::Signals,
    /// File-decision revision used to derive this verdict.
    pub expected_edit_revision: u64,
    /// The identity the verdict itself decides — a single settled match IS the
    /// pick, made by identification instead of by a click. `None` decides
    /// nothing (several matches, a conflict, nothing found).
    ///
    /// Either way it replaces whatever identification concluded last time: the
    /// pick belongs to the verdict that made it. A pick a person made outranks
    /// both and is left alone.
    pub identity_pick: Option<crate::import::IdentityPick>,
}

/// What identification concluded about one candidate. Present as a whole or
/// absent as a whole: the identify columns and the match rows below them are
/// written together and cleared together, so no reader has to reason about a
/// half-filled result.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct DbCandidateIdentifyResult {
    pub verdict: crate::identify::TerminalVerdict,
    pub probed_total_duration_ms: u64,
    pub identified_at: DateTime<Utc>,
}

/// One loaded `import_candidate_state` row, as
/// [`crate::db::Database::load_import_candidate_states`] returns it. Mirrors
/// the table: one key, and the two independent things derived under it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct DbImportCandidateState {
    pub content_hash: String,
    pub folder_path: String,
    /// What identification concluded, or `None` when nothing has identified
    /// this candidate yet — including when a file decision cleared what had,
    /// because that verdict described a folder shape that no longer applies.
    pub identify: Option<DbCandidateIdentifyResult>,
    /// What each of the candidate's audio units plays for. Empty when nothing
    /// has read them.
    pub durations: crate::import::probe::ProbedDurations,
    /// The signals identification settled on, or `None` when nothing has
    /// extracted them.
    pub signals: Option<crate::signals::Signals>,
    /// The user's decisions about this candidate's files: which audio each
    /// track sheet describes, and which files are the release's tracks.
    pub file_edits: crate::import::folder_scanner::CandidateFileEdits,
    /// The identity decided for this candidate, or `None` while nothing is
    /// decided. A person's choice survives file decisions and later verdicts
    /// alike — it names a release, not a shape; one identification concluded
    /// lives exactly as long as the verdict that concluded it.
    pub identity_pick: Option<crate::import::IdentityPick>,
}

/// Everything a person settled about one candidate through its pane, keyed by
/// the same content hash the rest of its state is.
///
/// Read as a group by the per-candidate query, which draws the header, the
/// cover and the mapping table from them at once. Kept off
/// [`DbImportCandidateState`] because that one is read for the whole queue and
/// none of these are.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbCandidatePaneRows {
    /// The cover the user chose. `None` leaves the picked release's default
    /// cover standing.
    pub cover: Option<crate::import::CoverSelection>,
    /// The album-level fields the user typed over the release's own.
    pub edit: crate::import::CandidateEditOverlay,
    /// The mapping-table rows the user changed or dropped.
    pub track_edits: Vec<crate::import::CandidateTrackEdit>,
    /// The last import of this candidate that failed.
    pub failure: Option<crate::import::ImportFailure>,
}

/// Every watched root with its status and every stored entry under it.
/// Only tests read whole snapshots; production reads entries by key.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct DbFolderScanSnapshot {
    pub watched_folder_path: String,
    pub generation: u64,
    pub status: crate::import::FolderScanStatus,
    pub items: Vec<crate::import::folder_scanner::ScanItem>,
}
