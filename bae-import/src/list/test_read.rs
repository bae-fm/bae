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

/// A list followed through one window: FollowedImportList reads each value
/// the window delivers in full, together with the count, summary, filter
/// entries and selection, so a test sees one snapshot per change to the rows
/// it holds. The test keeps the selected keys, as a surface does, and passes
/// them to each call that reads or changes what they select.
pub struct FollowedImportList {
    handle: crate::ImportServiceHandle,
    /// The request the list shows and the window it reads it through.
    shown: std::sync::Mutex<(super::ImportListRequest, bae_store::ListWindow)>,
    windows: std::sync::Mutex<
        std::sync::Arc<
            bae_store::live_query::CancellableLiveQuery<
                bae_store::ListWindow,
                super::ImportWindow,
                bae_store::DbError,
            >,
        >,
    >,
}

impl FollowedImportList {
    /// The list `request` shows, followed through `window`.
    pub(crate) fn follow(
        handle: crate::ImportServiceHandle,
        request: super::ImportListRequest,
        window: bae_store::ListWindow,
    ) -> Self {
        let windows = handle.subscribe_list_window(request.clone(), window.clone());
        Self {
            handle,
            shown: std::sync::Mutex::new((request, window)),
            windows: std::sync::Mutex::new(std::sync::Arc::new(windows)),
        }
    }

    fn request(&self) -> super::ImportListRequest {
        self.shown.lock().expect("the followed request").0.clone()
    }

    /// Show `view`, the folds unchanged, with `selected` the keys the test
    /// keeps selected; the change names where it opens and the ones the new
    /// view takes out.
    pub async fn set_view(
        &self,
        view: crate::ImportListView,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<super::ImportListViewChange, crate::ImportError> {
        let request = super::ImportListRequest { view, ..self.request() };
        self.show(request, selected).await
    }

    /// Show `request`, with `selected` the keys the test keeps selected.
    async fn show(
        &self,
        request: super::ImportListRequest,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<super::ImportListViewChange, crate::ImportError> {
        let change = self.handle.list_view_change(request.clone(), selected).await?;
        let window = self.shown.lock().expect("the followed request").1.clone();
        *self.windows.lock().expect("the followed window") =
            std::sync::Arc::new(self.handle.subscribe_list_window(request.clone(), window.clone()));
        *self.shown.lock().expect("the followed request") = (request, window);
        Ok(change)
    }

    /// The keys of every row the list shows: what Select All selects.
    pub async fn keys(&self) -> Result<Vec<crate::CandidateKey>, crate::ImportError> {
        self.handle.list_keys(self.request()).await
    }

    /// The rows of `selected` the list shows, in its order.
    pub async fn selected(
        &self,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<crate::selection::SelectedRows, crate::ImportError> {
        self.handle.list_selected(self.request(), selected, None).await
    }

    /// Read `window` from now on.
    pub fn set_window(&self, window: bae_store::ListWindow) {
        self.shown.lock().expect("the followed request").1 = window.clone();
        if self.windows.lock().expect("the followed window").set(window).is_err() {
            panic!("the list is open");
        }
    }

    /// Everything the list shows now, and what `selected` holds.
    pub async fn read(&self, selected: Vec<crate::CandidateKey>) -> ImportListRead {
        self.handle.read_list_for_test(self.request(), selected).await.expect("the list reads")
    }

    /// The next value of the window, with everything else the list shows and
    /// what `selected` holds read beside it.
    pub async fn next(&self, selected: Vec<crate::CandidateKey>) -> ImportListRead {
        let windows = self.windows.lock().expect("the followed window").clone();
        if windows.next().await.is_err() {
            panic!("the import list stays open");
        }
        self.read(selected).await
    }
}
