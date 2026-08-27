//! The import tab's list as it crosses: one window of items at a time, plus
//! the chrome around them.
//!
//! Mirrors `bae_core::import::list` field for field and decides nothing. Which
//! items exist, in what order, under which header and in which tab is core's;
//! a UI iterates a window and formats it for its locale.

use super::super::*;

/// The two orders the list offers, over the folder's path below its watched
/// root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportListOrder {
    PathAscending,
    PathDescending,
}

/// What the list is showing. A change to any of it changes which items sit at
/// which offsets, so it travels as one value.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportListView {
    pub tab: BridgeTriageTab,
    pub filter_text: String,
    /// The groups folded shut. Their entries are not in the list at all, which
    /// is why this is part of the request rather than a rendering decision.
    pub collapsed_groups: Vec<BridgeFolderReleaseDecisionKey>,
    pub order: BridgeImportListOrder,
}

/// One item at one offset. `stable_key` identifies it across reruns — the id a
/// paged list holds positions by.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportListItem {
    GroupHeader {
        stable_key: String,
        group: BridgeTriageGroup,
        watched_folder_path: String,
        expanded: bool,
        /// How many entries the group holds in this tab, after the filter.
        entry_count: u32,
    },
    Candidate {
        stable_key: String,
        row: BridgeTriageRow,
        is_group_member: bool,
    },
    Invalid {
        stable_key: String,
        invalid_candidate: BridgeInvalidCandidate,
        is_group_member: bool,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportListWindow {
    pub window: BridgeLibraryPageWindow,
    pub items: Vec<BridgeImportListItem>,
}

/// One Ready row, for the surfaces that act on the whole Ready set: the foot
/// bar's count, select-all, the bulk import's claims, and the covers to decode
/// before Pending opens.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReadyRowRef {
    pub candidate_key: String,
    /// The stored decision in the shape commit takes — a bulk import has no
    /// pane to read a claim line off.
    pub claim: BridgeIdentityChoice,
    pub cover_thumbnail_url: Option<String>,
}

/// The first candidate identification has not settled yet, and where that row
/// is in the requested view when the view includes it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFirstUnidentifiedRowRef {
    pub candidate_key: String,
    pub stable_key: String,
    pub group_key: Option<BridgeFolderReleaseDecisionKey>,
    pub visible_position: Option<u64>,
}

/// Everything the chrome around the list shows, computed in the same pass as
/// the items so none of it can drift from them.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportQueueSummary {
    /// `skipped` counts the Skipped rows **plus** the invalid folders.
    pub counts: BridgeTriageTabCounts,
    pub watched_folders: Vec<BridgeWatchedFolder>,
    pub folder_scan_statuses: Vec<BridgeWatchedFolderScanStatus>,
    /// Every group header the whole queue has, across all tabs — what
    /// disclosure state is retained against.
    pub group_keys: Vec<BridgeFolderReleaseDecisionKey>,
    /// The Ready rows matching the view's filter, in queue order.
    pub ready: Vec<BridgeReadyRowRef>,
    /// The first row the identify count is still waiting on, unfiltered, plus
    /// its position when the current view contains it.
    pub first_unidentified: Option<BridgeFirstUnidentifiedRowRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportListSnapshot {
    pub windows: Vec<BridgeImportListWindow>,
    pub total_count: u64,
    pub summary: BridgeImportQueueSummary,
    pub request_revision: u64,
    pub cause: BridgeLiveQueryCause,
}

/// One candidate as the pane reads it, whole: the folder with its files, what
/// the queue makes of it, the identify state it resumes, and everything the
/// pane draws — the picked release, the metadata form, the mapping table, the
/// cover, and the failure its last import left.
///
/// The pane holds no copy of any of it. Every control writes, and the next
/// value of this record is what redraws.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidateDetail {
    pub candidate: BridgeFolderCandidate,
    pub actionable: bool,
    /// The identify state the candidate's stored verdict stands back up as:
    /// what the pane shows when its runtime holds no run. `Idle` when nothing
    /// is stored for the candidate's current files.
    pub resumed_identify_state: BridgeIdentifyState,
    pub row: BridgeTriageRow,
    /// The picked release as its archived documents describe it. `None` with
    /// no pick, and for a folder read as its own tags.
    pub release: Option<BridgeReleaseDetail>,
    /// Whether the picked release is already in the library.
    pub picked_library_status: Option<BridgeLibraryStatus>,
    /// What identified the picked release, pinned to the candidate file each
    /// piece of evidence was read off — the chip that file's gallery tile or
    /// table row carries. Empty with no pick, for a folder read as its own
    /// tags, and where nothing identification matched on came from a file.
    pub file_evidence: Vec<BridgeFileEvidence>,
    /// The metadata form, seeded from the pick with the stored edits applied.
    /// `None` with no pick.
    pub edit: Option<BridgeRawReleaseEdit>,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: BridgeMappingTable,
    /// Audio units nothing has measured yet: their duration cells have no
    /// number to show and render as still being read.
    pub unprobed: Vec<BridgeAudioFile>,
    /// The cover this candidate commits with.
    pub cover: Option<BridgeCoverChoice>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<BridgeSignals>,
    /// The last import of this candidate that failed.
    pub failure: Option<BridgeImportFailure>,
}

/// An import that failed, as the pane still shows it after a relaunch.
///
/// `failed_at` is RFC 3339; the UI formats it in its own locale.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportFailure {
    pub error: String,
    pub failed_at: String,
}
