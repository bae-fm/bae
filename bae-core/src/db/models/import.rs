//! What the import queue derives about one candidate, keyed by the content
//! hash of its files: what identification concluded, and what a person
//! settled through its pane.

use super::*;

/// The current scan stamp for one candidate and whatever file-tag snapshot is
/// stored beneath it. The stored snapshot may carry an older stamp: callers
/// compare the two before reuse, while replacement uses the current pair as a
/// compare-and-set expectation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DbCandidateFileTagSnapshot {
    pub scan_generation: u64,
    pub candidate: crate::import::release_candidate::ReleaseCandidate,
    pub snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
}

/// What a caller supplies to record one candidate's identify verdict via
/// [`crate::db::Database::save_import_candidate_verdict`]. Normal outcomes use
/// `import_candidate_state`'s identify columns and failed outcomes use the
/// attached identify-failure row. Their timestamp is stamped by the write path
/// from the injected clock, the same convention as
/// `created_at` in `db/client/identity.rs`/`release.rs`: a timestamp that
/// records "when this write happened" is the DB layer's to assign, not data a
/// caller hands in — carrying it here would let a caller lie about it, and
/// would mean the sweep reaching for the ambient wall clock instead of the
/// fake-able one already threaded through `Database`.
///
/// It carries no file decisions: those are the user's half of the row and the
/// verdict write leaves them alone.
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
    /// Metadata revision identification began from. The write refuses a result
    /// when the editable draft changed while the run was in flight.
    pub expected_metadata_revision: u64,
    /// The editable metadata state this verdict concludes. A single match
    /// carries the projected release draft and its provenance; every other
    /// verdict carries the blank candidate draft. It replaces an earlier
    /// identification result as one unit, while a person's newer choice or
    /// edit wins through `expected_metadata_revision`.
    pub metadata: crate::import::CandidateMetadataDraft,
}

/// What identification concluded about one candidate. Present as a whole or
/// absent as a whole: normal identify columns, match rows, and the failed
/// verdict row are replaced or cleared in one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct DbCandidateIdentifyResult {
    pub verdict: crate::identify::TerminalVerdict,
    pub probed_total_duration_ms: u64,
    pub identified_at: DateTime<Utc>,
}

/// One loaded `import_candidate_state` row, as
/// [`crate::db::Database::load_import_candidate_states`] returns it. Mirrors
/// the table: one key, and the two independent things derived under it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbImportCandidateState {
    pub content_hash: String,
    pub folder_path: String,
    /// What identification concluded, or `None` when nothing has identified
    /// this candidate yet — including when a file decision cleared what had,
    /// because that verdict described a folder shape that no longer applies.
    pub identify: Option<DbCandidateIdentifyResult>,
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
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
    /// Revision of the editable metadata group. Every draft, artist, track, or
    /// cover mutation advances it so a source projection cannot overwrite a
    /// newer edit.
    pub metadata_revision: u64,
}

/// Everything a person settled about one candidate through its pane, keyed by
/// the same content hash the rest of its state is.
///
/// Read as a group by the per-candidate query, which draws the header, the
/// cover and the mapping table from them at once. Kept off
/// [`DbImportCandidateState`] because that one is read for the whole queue and
/// none of these are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCandidatePaneRows {
    /// The cover the user chose. `None` leaves the picked release's default
    /// cover standing.
    pub cover: Option<crate::import::CoverSelection>,
    /// The candidate's one stored editable metadata draft.
    pub metadata_draft: crate::import::RawReleaseEdit,
    /// Physical track decisions, independent of metadata replacement.
    pub(crate) track_mappings: Vec<crate::import::CandidateTrackMappingEdit>,
    /// Where the pane was when the person last left this candidate. `None`
    /// before the pane has been touched.
    pub session: Option<crate::import::CandidateSession>,
    /// The last import of this candidate that failed.
    pub failure: Option<crate::import::ImportFailure>,
}

/// One atomic read of the existing candidate rows the import worker consumes.
/// The revision is checked against the queued expectation before any source
/// file is read.
#[derive(Debug, Clone, PartialEq)]
pub struct DbCandidateImportPreparation {
    pub file_edit_revision: u64,
    pub metadata_revision: u64,
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
    pub cover: Option<crate::import::CoverSelection>,
    pub metadata_draft: crate::import::RawReleaseEdit,
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub(crate) track_mappings: Vec<crate::import::CandidateTrackMappingEdit>,
    pub assets: crate::import::CandidatePreparedAssets,
}

/// The exact candidate state a library import transaction is allowed to
/// consume. The final write checks this inside the transaction before writing
/// any library row.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub(crate) enum ImportCommitGuard {
    Candidate {
        candidate_key: String,
        source: crate::import::release_candidate::CandidateSource,
        expectation: crate::import::service::ImportExpectation,
    },
    #[cfg(test)]
    UncheckedTestSetup,
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
