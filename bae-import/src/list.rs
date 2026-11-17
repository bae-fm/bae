//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! Each surface's list is a row of its own (`import_list`), holding its view
//! and the groups it folds shut, and what the list shows is a query over that
//! row and the placement the import tables keep (`import_list_placement`).
//! Each window of it is a live query of its own whose request is a range of
//! the list's order, and the count, the summary, Found's filter entries and
//! what the selection offers are live queries of their own: coven reruns each
//! live query whose reads a write changed. Changing the view or folding a
//! group is a write, and coven reruns the live queries it changes. The surface
//! keeps the rows the person selects, and passes their keys to the selection's
//! live query and to each action on them.
//!
//! What is running for a candidate right now — a run queued or in flight, an
//! import that owns it — is in the standings table the candidate runtime
//! writes, and the placement's Found state already counts it: each row carries
//! it as its [`CandidateLiveState`], and the selection's offers read it.

use super::folder_scanner::FolderReleaseDecisionKey;
use super::triage::{CandidateLiveState, InvalidRow, TriageGroup, TriageRow};
use bae_store::WindowRead;

mod path_order;
mod pending_filter;
mod subscription;
#[cfg(any(test, feature = "test-utils"))]
mod test_read;

pub(crate) use path_order::release_keys_in_path_order;
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
        /// for a member of a group. The list's query reads this key with the
        /// row, so a surface knows the row's header as soon as it has the
        /// row, whichever window holds the header.
        header_key: Option<String>,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it. It offers nothing to do, and nothing runs for it.
    Invalid { row: InvalidRow },
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
            Self::Invalid { row, .. } => {
                format!("invalid:{}", hex::encode(coven::path_bytes(&row.folder)))
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
}

/// Whether bae watches any folder, whether Found has groups, and the
/// narrowing, which a surface shows beside the list's rows and counts. The
/// summary has its own live query, so the rows a scan writes do not rerun it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    /// Whether bae watches any folder: with none, the surface shows no list.
    pub has_watched_folders: bool,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
    /// What the view asked the rows to be narrowed by when this read was
    /// made: what the count's `narrowed` counts under, so a surface shows
    /// the count only beside the filter it is for.
    pub narrowing: ImportListNarrowing,
}

/// How many items the list holds, how many of its tab's entries its
/// narrowing keeps, and how many entries each tab holds. The count has its own
/// live query because every scan commit changes it; the surface sizes the
/// list's scroll view from it, and the tab bar shows the tabs' counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportListCount {
    pub rows: u64,
    /// How many of the tab's entries the list shows, of how many it has,
    /// while the text filter or the filter entry narrows it; `None` while
    /// nothing does.
    pub narrowed: Option<NarrowedCount>,
    /// Every tab's count, whether or not the text filter or the filter entry
    /// narrows the list.
    pub tabs: crate::TriageTabCounts,
}

/// Where a list's view opens once it is set: what a list narrowed anew shows
/// on top.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListOpening {
    /// The list's top: the view shows none of the selected rows.
    Top,
    /// The first selected row the new view shows, as the write that set the
    /// view found it: the position the row sits at, where the list's view
    /// opens.
    At { position: u64 },
}

/// What setting a list's view came to: where the new view opens, and the
/// selected rows it takes out of the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListViewChange {
    pub opening: ImportListOpening,
    /// The keys of the selected rows that leave the selection: on another
    /// tab, filter or search, each selected row the new view doesn't show,
    /// and none on another order.
    pub left: Vec<crate::CandidateKey>,
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
