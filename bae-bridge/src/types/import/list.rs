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
    },
    Boundary {
        stable_key: String,
        boundary: BridgeFolderReleaseBoundary,
    },
    Invalid {
        stable_key: String,
        invalid_candidate: BridgeInvalidCandidate,
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
    /// The first row the identify count is still waiting on.
    pub first_unidentified_key: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportListSnapshot {
    pub windows: Vec<BridgeImportListWindow>,
    pub total_count: u64,
    pub summary: BridgeImportQueueSummary,
    pub request_revision: u64,
    pub cause: BridgeLiveQueryCause,
}

/// One candidate as the pane reads it: the folder with its files, what the
/// queue makes of it, and the identify state it resumes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidateDetail {
    pub candidate: BridgeFolderCandidate,
    pub actionable: bool,
    /// The identify state the candidate's stored verdict stands back up as:
    /// what the pane shows when its runtime holds no run. `Idle` when nothing
    /// is stored for the candidate's current files.
    pub resumed_identify_state: BridgeIdentifyState,
    pub row: BridgeTriageRow,
}
