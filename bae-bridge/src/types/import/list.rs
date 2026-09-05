//! The import tab's list as it crosses: one window of items at a time, plus
//! the chrome around them.
//!
//! Mirrors `bae_core::import::list` field for field and decides nothing. Which
//! items exist, in what order, under which header and in which tab is core's;
//! a UI iterates a window and formats it for its locale.

use super::super::*;

/// Folder dates or natural folder-path order. Done dates are import dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportListOrder {
    NewestFirst,
    OldestFirst,
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

/// The list view and position that reveal one candidate at its current
/// placement.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidateListLocation {
    pub stable_key: String,
    pub tab: BridgeTriageTab,
    pub group_key: Option<BridgeFolderReleaseDecisionKey>,
    pub visible_position: u64,
}

/// Everything the chrome around the list shows, computed in the same pass as
/// the items so none of it can drift from them.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportQueueSummary {
    /// `skipped` counts the Skipped rows **plus** the invalid folders.
    pub counts: BridgeTriageTabCounts,
    pub watched_folders: Vec<BridgeWatchedFolder>,
    pub folder_scan_statuses: Vec<BridgeWatchedFolderScanStatus>,
    pub folder_scan_activity: Option<BridgeFolderScanActivity>,
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
pub struct BridgeFolderScanActivity {
    pub found_count: u64,
    pub folders: Vec<BridgeActiveFolderScan>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeActiveFolderScan {
    pub watched_folder_path: String,
    pub watched_folder_name: String,
    pub found_count: u64,
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
    /// Extracted identifying signals pinned to their source files. Independent
    /// of the selected pressing; support for a search result is carried by its
    /// result provenance instead.
    pub file_evidence: Vec<BridgeFileEvidence>,
    /// The candidate's one editable metadata draft.
    pub metadata_draft: BridgeRawReleaseEdit,
    /// Whether the draft contains no authored or sourced metadata.
    pub metadata_draft_is_blank: bool,
    /// Where the current draft began, absent for direct entry and after clear.
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
    /// Revision of the exact metadata draft and selected cover in this value.
    pub metadata_revision: u64,
    /// Source policy captured when this candidate was first discovered.
    pub initial_metadata_source: BridgeDefaultImportMetadataSource,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: BridgeMappingTable,
    /// The cover this candidate commits with.
    pub cover: Option<BridgeCoverChoice>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<BridgeSignals>,
    /// The last import of this candidate that failed.
    pub failure: Option<BridgeImportFailure>,
}

/// An import that failed, as the pane still shows it after a relaunch.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportFailure {
    pub error: BridgeError,
    pub artist_identity_conflict: Option<BridgeArtistIdentityConflict>,
}

/// The two library rows an incoming cross-provider artist identity connected.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistIdentityConflict {
    pub incoming_artist_name: String,
    pub discogs_artist: BridgeExistingArtist,
    pub musicbrainz_artist: BridgeExistingArtist,
}
