//! The whole of a list as one test reads it: every item through one window
//! (`windows` holds that one read),
//! and the count, summary, filter entries and selection beside them, read in
//! one snapshot so they agree with each other.

use super::{ImportListCount, ImportListItem, ImportQueueSummary, PendingFilterEntry};
use crate::selection::SelectionSummary;
use bae_store::WindowRead;

/// What a list shows, read once.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListRead {
    pub windows: Vec<WindowRead<ImportListItem>>,
    pub count: ImportListCount,
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
        Self { windows: vec![window], count, summary, pending_filter_entries, selection }
    }
}

/// A list followed through one window: the request it shows and the live
/// query of the window it reads it through. A test waits for the window's
/// next value with [`Self::changed`], and reads everything else the list
/// shows through the import handle under [`Self::request`], so the list holds
/// only its live query and the request it runs under. The test keeps the
/// selected keys, as a surface does, and passes them to each read of what
/// they select.
pub struct FollowedImportList {
    request: super::ImportListRequest,
    windows: bae_store::live_query::CancellableLiveQuery<
        bae_store::ListWindow,
        super::ImportWindow,
        bae_store::DbError,
    >,
}

impl FollowedImportList {
    /// The list `request` shows, followed through `windows`, a live query of
    /// its window under that request.
    pub(crate) fn follow(
        request: super::ImportListRequest,
        windows: bae_store::live_query::CancellableLiveQuery<
            bae_store::ListWindow,
            super::ImportWindow,
            bae_store::DbError,
        >,
    ) -> Self {
        Self { request, windows }
    }

    /// The request the list shows.
    pub fn request(&self) -> &super::ImportListRequest {
        &self.request
    }

    /// Read `window` from now on.
    pub fn set_window(&self, window: bae_store::ListWindow) {
        if self.windows.set(window).is_err() {
            panic!("the list is open");
        }
    }

    /// Wait for the window's next value.
    pub async fn changed(&self) {
        if self.windows.next().await.is_err() {
            panic!("the import list stays open");
        }
    }
}
