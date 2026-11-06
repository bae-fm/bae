//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! Each surface's list is a row of its own (`import_list`), holding its view,
//! the groups it folds shut and the rows it has selected, and what the list
//! shows is a query over that row and the placement the import tables keep
//! (`import_list_placement`): the requested windows, the tab counts, Found's
//! filter entries and what the selection offers all come from one read, so
//! none of it can disagree with the rows. Changing the view, folding a group
//! or selecting rows is a write the list's live query answers; the windows
//! are the query's request.
//!
//! What is running for a candidate right now — a run queued or in flight, an
//! import that owns it — is in the standings table the candidate runtime
//! writes, and the placement's Found state already counts it: each row carries
//! it as its [`CandidateLiveState`], and the selection's offers read it.

use super::folder_scanner::{FolderReleaseDecisionKey, InvalidCandidate};
use super::triage::{CandidateLiveState, TriageGroup, TriageRow, TriageTabCounts};
use super::watched_folder::WatchedFolder;
use bae_store::{LibraryBrowseWindow, LibraryPageWindows};

mod path_order;
mod pending_filter;
mod subscription;

pub(crate) use path_order::release_keys_in_path_order;
pub(crate) use pending_filter::pending_filter_entries;
pub use pending_filter::{PendingFilter, PendingFilterEntry};
pub use subscription::{LiveImportList, LiveImportListError};

pub use super::triage::TriageTab;

/// What the list is currently showing: which tab, which filters, and in which
/// direction. Which groups it folds shut is the list's too, written on their
/// own ([`LiveImportList::set_group_folded`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListView {
    pub tab: TriageTab,
    pub filter_text: String,
    /// Which of Found's rows the list shows. Done and Skipped rows are past
    /// identification, so it leaves them alone.
    pub pending_filter: PendingFilter,
    pub order: ImportListOrder,
}

impl Default for ImportListView {
    fn default() -> Self {
        Self {
            tab: TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
            order: ImportListOrder::NewestFirst,
        }
    }
}

/// Folder dates or natural folder-path order, never the editable album title.
/// Done uses the library import date and keeps outstanding uploads first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportListOrder {
    NewestFirst,
    OldestFirst,
    PathAscending,
    PathDescending,
}

/// One list a surface has open: the id of its `import_list` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportListId(pub(crate) i64);

/// What one read of a list answers: the list, and the windows of it to load.
/// Everything else the read is a function of is in the tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListRequest {
    pub list: ImportListId,
    pub windows: LibraryPageWindows,
}

/// One item in the list, at one offset.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListItem {
    GroupHeader {
        group: TriageGroup,
        watched_folder_path: std::path::PathBuf,
        expanded: bool,
        /// How many entries the group holds in this tab, after the filter.
        entry_count: u32,
    },
    /// A candidate, presented as what the candidate reads as on whichever tab
    /// places it. A Done row is the row it was in Found, its import status
    /// naming the one release this device imported from its files; it is
    /// never a group member, since only Pending rows join a group.
    Candidate {
        row: TriageRow,
        /// What is running for the candidate, as the read found it, and the
        /// commands and state the row has with it.
        live: CandidateLiveState,
        /// Where the header of the group it is listed under sits in the
        /// list, for a member of a group: before it, and before every member
        /// between them. Read with the row whichever window holds the
        /// header, so the header the row sits under is known as soon as the
        /// row is.
        header_position: Option<u64>,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it. It offers nothing to do, and nothing runs for it.
    Invalid {
        candidate: InvalidCandidate,
        /// Whether the person has selected the row.
        selected: bool,
    },
}

/// The stable key of a candidate's item, whichever tab places it.
pub(crate) fn candidate_stable_key(candidate_key: &crate::CandidateKey) -> String {
    format!("candidate:{}", hex::encode(candidate_key.as_bytes()))
}

impl ImportListItem {
    /// Stable identity for one item, from the bytes that key it, so two
    /// folders whose names show alike stay apart. Variant prefixes keep a
    /// candidate, a boundary and a group header at the same folder from
    /// sharing view state; hex holds no `:`, so a two-part key is
    /// unambiguous.
    pub fn stable_key(&self) -> String {
        match self {
            Self::GroupHeader { group, .. } => format!(
                "group:{}:{}",
                hex::encode(coven::path_bytes(&group.key.watched_folder_path)),
                hex::encode(group.key.relative_folder.as_bytes())
            ),
            Self::Candidate { row, .. } => candidate_stable_key(&row.candidate_key),
            Self::Invalid { candidate, .. } => {
                format!("invalid:{}", hex::encode(coven::path_bytes(&candidate.path)))
            }
        }
    }
}

/// The list view and position that reveal one candidate at its current
/// placement.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateListLocation {
    pub stable_key: String,
    pub tab: TriageTab,
    /// The Pending group that has to be open for this candidate to be visible.
    pub group_key: Option<FolderReleaseDecisionKey>,
    pub visible_position: u64,
}

/// Everything the chrome around the list shows, read with the items so none
/// of it can drift from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub counts: TriageTabCounts,
    pub watched_folders: Vec<WatchedFolder>,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
    /// How many of the tab's entries the list shows, of how many it has,
    /// while the text filter or the filter entry narrows it; `None` while
    /// nothing does.
    pub narrowed: Option<NarrowedCount>,
    /// What the view asked the rows to be narrowed by when this read was
    /// made: what `narrowed` counts under, so a surface shows the count only
    /// beside the filter it is for.
    pub narrowing: ImportListNarrowing,
    /// Where the list's view opened: the position its first selected row had
    /// in it when the view was set, or its top (0) while it showed no
    /// selected row. What a list narrowed anew shows on top.
    pub opened_at: u64,
}

/// What a view narrows its rows by: the tab, the text filter and the filter
/// entry, as the view states them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListNarrowing {
    pub tab: TriageTab,
    pub filter_text: String,
    pub pending_filter: PendingFilter,
}

/// How many of a tab's entries a narrowed list shows, of how many it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NarrowedCount {
    pub shown: u32,
    pub total: u32,
}

/// How far the identifications and imports in flight have got, for the
/// chrome around the list, whoever started them. Read apart from the list, by
/// its own live query: each identification's ending moves the count and
/// nothing the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportCounts {
    /// How many identifications of the batch in flight have ended, of
    /// `admitted`; both zero when no batch is in flight.
    pub identified: u32,
    pub admitted: u32,
    /// How many candidates an import holds, waiting for the worker or running.
    pub imports_in_flight: u32,
}

/// One read of the list: the requested windows, the total, and the chrome, as
/// the tables hold them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListProjection {
    pub windows: Vec<LibraryBrowseWindow<ImportListItem>>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    /// Every entry of Found's filter, in the menu's order, with how many of
    /// Found's rows it holds, whatever the view shows.
    pub pending_filter_entries: Vec<PendingFilterEntry>,
    /// What the selection holds and can be told to do, with what is running
    /// for each selected row.
    pub selection: crate::selection::SelectionSummary,
    /// The changes to the list's view, its folds included, this read
    /// reflects.
    pub view_revision: u64,
    /// The changes to the list's selection this read reflects: the person's,
    /// and those that took out a row the list stopped showing.
    pub selection_revision: u64,
}

/// A projection, and what woke the live query.
#[derive(Debug, Clone)]
pub struct ImportListSnapshot {
    pub windows: Vec<LibraryBrowseWindow<ImportListItem>>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    /// Every entry of Found's filter, in the menu's order, with how many of
    /// Found's rows it holds.
    pub pending_filter_entries: Vec<PendingFilterEntry>,
    /// The changes to the selection the rows' `selected` reflects.
    pub selection_revision: u64,
    /// What the selection holds and can be told to do: the selected rows
    /// this snapshot shows, with what is running for them.
    pub selection: crate::selection::SelectionSummary,
    /// The changes to the view, its folds included, the rows reflect.
    pub view_revision: u64,
    pub cause: coven::LiveQueryCause,
}
