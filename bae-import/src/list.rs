//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! Each surface's list is a row of its own (`import_list`), holding its view,
//! the groups it folds shut and the rows it has selected, and what the list
//! shows is a query over that row and the placement the import tables keep
//! (`import_list_placement`). Each window of it is a live query of its own
//! whose request is a range of the list's order, and the count, the summary,
//! Found's filter entries and what the selection offers are live queries of
//! their own: a write reruns what it changed. Changing the view, folding a
//! group or selecting rows is a write the list's live queries answer.
//!
//! What is running for a candidate right now — a run queued or in flight, an
//! import that owns it — is in the standings table the candidate runtime
//! writes, and the placement's Found state already counts it: each row carries
//! it as its [`CandidateLiveState`], and the selection's offers read it.

use super::folder_scanner::{FolderReleaseDecisionKey, InvalidCandidate};
use super::triage::{CandidateLiveState, TriageGroup, TriageRow};
use super::watched_folder::WatchedFolder;
use bae_store::list_windows::OrderKey;
use bae_store::WindowRead;

mod path_order;
mod pending_filter;
mod subscription;
#[cfg(any(test, feature = "test-utils"))]
mod test_read;

pub(crate) use path_order::release_keys_in_path_order;
pub(crate) use pending_filter::pending_filter_entries;
pub use pending_filter::{PendingFilter, PendingFilterEntry};
pub use subscription::{ImportListFailure, LiveImportList};
#[cfg(any(test, feature = "test-utils"))]
pub use test_read::{FollowedImportList, ImportListRead};

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

/// One item in the list.
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
        /// The stable key of the header of the group it is listed under,
        /// for a member of a group. Read with the row, so the header the
        /// row sits under is known as soon as the row is, whichever window
        /// holds the header.
        header_key: Option<String>,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it. It offers nothing to do, and nothing runs for it.
    Invalid {
        candidate: InvalidCandidate,
        /// Whether the person has selected the row.
        selected: bool,
    },
}

/// The stable key of a group's header item.
pub(crate) fn group_stable_key(group: &FolderReleaseDecisionKey) -> String {
    format!(
        "group:{}:{}",
        hex::encode(coven::path_bytes(&group.watched_folder_path)),
        hex::encode(group.relative_folder.as_bytes())
    )
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
            Self::GroupHeader { group, .. } => group_stable_key(&group.key),
            Self::Candidate { row, .. } => candidate_stable_key(&row.candidate_key),
            Self::Invalid { candidate, .. } => {
                format!("invalid:{}", hex::encode(coven::path_bytes(&candidate.path)))
            }
        }
    }
}

/// The list view and place that reveal one candidate at its current
/// placement.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateListLocation {
    pub stable_key: String,
    pub tab: TriageTab,
    /// The Pending group that has to be open for this candidate to be visible.
    pub group_key: Option<FolderReleaseDecisionKey>,
    pub visible_position: u64,
    /// Where the candidate sits in that view, as a window's bound: a chain of
    /// windows that starts at it shows it.
    pub key: OrderKey,
}

/// What the chrome around the list shows beside its rows and counts. Its
/// own live query: a scan's rows move none of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub watched_folders: Vec<WatchedFolder>,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
    /// What the view asked the rows to be narrowed by when this read was
    /// made: what the count's `narrowed` counts under, so a surface shows
    /// the count only beside the filter it is for.
    pub narrowing: ImportListNarrowing,
}

/// How many items the list holds, and how many of its tab's entries its
/// narrowing keeps. Its own live query: the count that sizes the list's
/// scroll view, which every scan commit moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportListCount {
    pub rows: u64,
    /// How many of the tab's entries the list shows, of how many it has,
    /// while the text filter or the filter entry narrows it; `None` while
    /// nothing does.
    pub narrowed: Option<NarrowedCount>,
}

/// Where a list's view opens once it is set: what a list narrowed anew shows
/// on top.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListOpening {
    /// The list's top: the view showed no selected row.
    Top,
    /// Its first selected row, as the write that set the view found it: the
    /// key a chain of windows starts at, and the position the row sits at.
    At { key: OrderKey, position: u64 },
}

/// One window of the list as it is read: its items, and the header of the
/// group the first of them is a member of when the window starts inside the
/// group, so a pinned header has it whichever windows are loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportWindow {
    pub read: WindowRead<ImportListItem>,
    pub leading_header: Option<ImportListItem>,
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
