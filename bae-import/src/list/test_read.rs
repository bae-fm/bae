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

/// A list followed through one window: each value the window delivers is
/// read whole, with the count, summary, filter entries and selection read
/// beside it, so a test sees one snapshot per change to the rows it holds.
pub struct FollowedImportList {
    list: super::LiveImportList,
    windows: bae_store::live_query::CancellableLiveQuery<
        bae_store::ListWindow,
        super::ImportWindow,
        bae_store::DbError,
    >,
}

impl FollowedImportList {
    /// Show `view` in the list.
    pub async fn set_view(
        &self,
        view: crate::ImportListView,
    ) -> Result<Option<crate::ImportListOpening>, super::ImportListFailure> {
        self.list.set_view(view).await
    }

    /// Apply `change` to the list's selection.
    pub async fn change_selection(
        &self,
        change: crate::selection::SelectionChange,
    ) -> Result<(), super::ImportListFailure> {
        self.list.change_selection(change).await
    }

    /// Select every row the list shows.
    pub async fn select_all(&self) -> Result<(), super::ImportListFailure> {
        self.list.select_all().await
    }

    /// The selected rows, in the order the list shows them.
    pub async fn selected(
        &self,
    ) -> Result<crate::selection::SelectedRows, super::ImportListFailure> {
        self.list.selected().await
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

    /// The next value of the window, with everything else the list shows read
    /// beside it.
    pub async fn next(&self) -> ImportListRead {
        if self.windows.next().await.is_err() {
            panic!("the import list stays open");
        }
        self.list.read_for_test().await
    }
}
