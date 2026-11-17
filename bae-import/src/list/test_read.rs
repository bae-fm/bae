//! The whole of a list as one test reads it: every item through one window
//! (`windows` holds that one read),
//! and the count, summary, filter entries and selection beside them, read in
//! one snapshot so they agree with each other.

use super::{
    ImportListCount, ImportListItem, ImportQueueSummary, NarrowedCount, PendingFilterEntry,
};
use crate::selection::SelectionSummary;
use bae_store::WindowRead;

/// What a list shows, read once.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListRead {
    pub windows: Vec<WindowRead<ImportListItem>>,
    pub total_count: u64,
    pub narrowed: Option<NarrowedCount>,
    pub summary: ImportQueueSummary,
    pub pending_filter_entries: Vec<PendingFilterEntry>,
    pub selection: SelectionSummary,
}

impl ImportListRead {
    pub(crate) fn of(
        window: WindowRead<ImportListItem>,
        count: ImportListCount,
        summary: ImportQueueSummary,
        pending_filter_entries: Vec<PendingFilterEntry>,
        selection: SelectionSummary,
    ) -> Self {
        Self {
            windows: vec![window],
            total_count: count.rows,
            narrowed: count.narrowed,
            summary,
            pending_filter_entries,
            selection,
        }
    }
}

/// A list followed through one window: FollowedImportList reads each value
/// the window delivers in full, together with the count, summary, filter
/// entries and selection, so a test sees one snapshot per change to the rows
/// it holds. The test keeps the selected keys, as a surface does, and passes
/// them to each call that reads or changes what they select.
pub struct FollowedImportList {
    list: super::LiveImportList,
    windows: bae_store::live_query::CancellableLiveQuery<
        bae_store::ListWindow,
        super::ImportWindow,
        bae_store::DbError,
    >,
}

impl FollowedImportList {
    /// Show `view` in the list, with `selected` the keys the test keeps
    /// selected; the change names the ones the new view takes out.
    pub async fn set_view(
        &self,
        view: crate::ImportListView,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<Option<crate::ImportListViewChange>, super::ImportListFailure> {
        self.list.set_view(view, selected).await
    }

    /// The keys of every row the list shows: what Select All selects.
    pub async fn keys(&self) -> Result<Vec<crate::CandidateKey>, super::ImportListFailure> {
        self.list.keys().await
    }

    /// The rows of `selected` the list shows, in its order.
    pub async fn selected(
        &self,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<crate::selection::SelectedRows, super::ImportListFailure> {
        self.list.selected(selected).await
    }

    /// `list` followed through `window`.
    pub fn follow(list: super::LiveImportList, window: bae_store::ListWindow) -> Self {
        let windows = list.until_closed(list.subscribe_window(window));
        Self { list, windows }
    }

    /// Read `window` from now on.
    pub fn set_window(&self, window: bae_store::ListWindow) {
        if self.windows.set(window).is_err() {
            panic!("the list is open");
        }
    }

    /// Everything the list shows now, and what `selected` holds.
    pub async fn read(&self, selected: Vec<crate::CandidateKey>) -> ImportListRead {
        self.list.read_for_test(selected).await
    }

    /// The next value of the window, with everything else the list shows and
    /// what `selected` holds read beside it.
    pub async fn next(&self, selected: Vec<crate::CandidateKey>) -> ImportListRead {
        if self.windows.next().await.is_err() {
            panic!("the import list stays open");
        }
        self.list.read_for_test(selected).await
    }
}
