//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! The surface keeps the list's view and the groups it folds shut, and each
//! read of the list takes them as its request ([`ImportListRequest`]): what
//! the list shows is a query over that request and the placement the import
//! tables keep (`import_list_placement`). Each window of it is a live query of
//! its own whose request is a range of the list's order, and the count, the
//! summary, Found's filter entries and what the selection offers are live
//! queries of their own: coven reruns each live query whose reads a write
//! changed. Showing another view or folding a group is the surface opening new
//! reads under a new request. The surface keeps the rows the person selects,
//! and passes their keys to the selection's live query and to each action on
//! them.
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
#[cfg(any(test, feature = "test-utils"))]
mod test_read;

pub(crate) use path_order::release_keys_in_path_order;
pub use pending_filter::{PendingFilter, PendingFilterEntry};
#[cfg(any(test, feature = "test-utils"))]
pub use test_read::{FollowedImportList, ImportListRead};

pub use super::triage::TriageTab;

/// What the list is currently showing: which tab, which filters, and in which
/// direction. Which groups it folds shut is not part of it: the request holds
/// those beside it ([`ImportListRequest`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListView {
    pub tab: TriageTab,
    /// The text the person searches the rows for, `None` while they search
    /// for nothing.
    pub search: Option<SearchText>,
    /// Which of Found's rows the list shows. Done and Skipped rows are past
    /// identification, so it leaves them alone.
    pub pending_filter: PendingFilter,
    pub order: ImportListOrder,
}

impl Default for ImportListView {
    fn default() -> Self {
        Self {
            tab: TriageTab::Pending,
            search: None,
            pending_filter: PendingFilter::All,
            order: ImportListOrder::NewestFirst,
        }
    }
}

/// Text the person searches the list's rows for. It is never empty: a list
/// the person searches for no text holds no `SearchText`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchText(String);

impl SearchText {
    /// The search for `text`, or `None` when the person typed no text.
    pub fn of(text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        (!text.is_empty()).then_some(Self(text))
    }

    /// The text as the person typed it.
    pub fn as_str(&self) -> &str {
        &self.0
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

/// What every read of the import list takes: the view the surface shows and
/// the groups it folds shut. The surface owns both, and each read builds its
/// statement from them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportListRequest {
    pub view: ImportListView,
    /// The groups the list folds shut: their members are not in it. A group
    /// that leaves the list and comes back under the same folder comes back
    /// folded.
    pub folded: std::collections::BTreeSet<FolderReleaseDecisionKey>,
}

impl From<ImportListView> for ImportListRequest {
    /// The request showing `view` with no group folded.
    fn from(view: ImportListView) -> Self {
        Self { view, folded: Default::default() }
    }
}

impl ImportListRequest {
    /// The request showing `view` with no group folded.
    pub fn of(view: ImportListView) -> Self {
        view.into()
    }

    /// Whether the text filter narrows the tab, which only a read of every
    /// row's text can count.
    pub(crate) fn filters_text(&self) -> bool {
        self.view.search.is_some()
    }

    /// Found's filter entry while it narrows the list: `None` on another tab,
    /// or while the entry keeps every row.
    pub(crate) fn entry(&self) -> Option<PendingFilter> {
        match (self.view.tab, self.view.pending_filter) {
            (TriageTab::Done | TriageTab::Skipped, _)
            | (TriageTab::Pending, PendingFilter::All) => None,
            (TriageTab::Pending, entry) => Some(entry),
        }
    }

    /// Whether the text filter or Found's filter entry narrows the tab.
    pub(crate) fn narrows(&self) -> bool {
        self.filters_text() || self.entry().is_some()
    }

    /// Whether the list shows every row of its tab: nothing narrows it and no
    /// group is folded shut.
    pub(crate) fn keeps_all(&self) -> bool {
        !self.narrows() && self.folded.is_empty()
    }
}

/// One item in the list.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListItem {
    GroupHeader(ImportGroupHeader),
    /// A candidate, presented as what the candidate reads as on whichever tab
    /// places it. A Done row is the row it was in Found, its import status
    /// naming the one release this device imported from its files; it is
    /// never a group member, since only Pending rows join a group.
    Candidate {
        row: TriageRow,
        /// What is running for the candidate, as the read found it, and the
        /// commands and state the row has with it.
        live: CandidateLiveState,
        /// The group it is listed under, for a member of a group. The
        /// list's query reads the group's header with the row, so a surface
        /// has the row's header as soon as it has the row, whichever window
        /// holds the header's own row.
        group: Option<ImportGroupMember>,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it. It offers nothing to do, and nothing runs for it.
    Invalid {
        row: InvalidRow,
    },
}

/// A group's header: the group, how the list shows it, and how many of its
/// entries the list holds.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportGroupHeader {
    pub group: TriageGroup,
    pub watched_folder_path: std::path::PathBuf,
    pub expanded: bool,
    /// How many entries the group holds in this tab, after the filter.
    pub entry_count: u32,
}

/// A release's place in the group it is listed under: the group's header,
/// and whether the release is the last member the list shows of the group.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportGroupMember {
    pub header: ImportGroupHeader,
    pub last: bool,
}

impl ImportGroupHeader {
    /// The stable key of the header's item.
    pub fn stable_key(&self) -> String {
        group_stable_key(&self.group.key)
    }

    /// Whether the list shows no member under the header, so the header's
    /// row is the last row the list shows of its group: the list folds the
    /// group shut, or keeps none of its releases.
    pub fn shows_no_member(&self) -> bool {
        !self.expanded || self.entry_count == 0
    }
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
            Self::GroupHeader(header) => header.stable_key(),
            Self::Candidate { row, .. } => candidate_stable_key(&row.candidate_key),
            Self::Invalid { row, .. } => {
                format!("invalid:{}", hex::encode(coven::path_bytes(&row.folder)))
            }
        }
    }
}

/// The list view that reveals one candidate at its current placement: its
/// tab, with the filters cleared and the group `group_key` opened.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateListLocation {
    pub tab: TriageTab,
    /// The Pending group that has to be open for this candidate to be visible.
    pub group_key: Option<FolderReleaseDecisionKey>,
}

/// Whether bae watches any folder and whether Found has groups, which a
/// surface shows beside the list's rows. The summary has its own live query,
/// so the rows a scan writes do not rerun it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    /// Whether bae watches any folder: with none, the surface shows no list.
    pub has_watched_folders: bool,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
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

/// Where a list's view opens: what a list narrowed anew shows on top.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListOpening {
    /// The list's top: the view shows none of the selected rows.
    Top,
    /// The first selected row the new view shows: the position the row sits
    /// at, where the list's view opens.
    At { position: u64 },
}

/// What showing another view comes to: where the new view opens, and the
/// selected rows it takes out of the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListViewChange {
    pub opening: ImportListOpening,
    /// The keys of the selected rows that leave the selection: on another
    /// tab, filter or search, each selected row the new view doesn't show,
    /// and none on another order.
    pub left: Vec<crate::CandidateKey>,
}

/// One window of the list as it is read: its items, each with its key.
pub type ImportWindow = WindowRead<ImportListItem>;

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
