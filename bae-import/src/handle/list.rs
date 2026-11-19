//! The import list, the scans and the candidates as a surface reads them
//! over the store.

use super::*;

impl ImportServiceHandle {
    /// Where a bug a surface's live query carries goes as it arrives: the
    /// crash reporter.
    fn bugs(&self) -> coven::task::BugSinkRef {
        self.services.diagnostics.bug_sink()
    }

    /// One window of the list `request` shows as a live query whose window
    /// the caller changes on the same query. It ends when the library
    /// closes.
    pub fn subscribe_list_window(
        &self,
        request: crate::ImportListRequest,
        initial: bae_store::ListWindow,
    ) -> bae_store::live_query::CancellableLiveQuery<
        bae_store::ListWindow,
        crate::ImportWindow,
        bae_store::DbError,
    > {
        self.until_closed(self.services.tables.subscribe_import_window(request, initial))
    }

    /// How many items the list `request` shows holds and how many its
    /// narrowing keeps, live.
    pub fn subscribe_list_count(
        &self,
        request: crate::ImportListRequest,
    ) -> coven::LiveQuery<crate::ImportListCount, bae_store::DbError> {
        self.services.tables.subscribe_import_count(request)
    }

    /// How many items of the list `request` shows sit before the one at a
    /// key, as a live query whose key the caller changes on the same query.
    /// It ends when the library closes.
    pub fn subscribe_list_position(
        &self,
        request: crate::ImportListRequest,
        initial: bae_store::list_windows::OrderKey,
    ) -> bae_store::live_query::CancellableLiveQuery<
        bae_store::list_windows::OrderKey,
        u64,
        bae_store::DbError,
    > {
        self.until_closed(self.services.tables.subscribe_import_position(request, initial))
    }

    /// Whether bae watches any folder and whether Found has groups, live.
    pub fn subscribe_list_summary(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::ImportQueueSummary> {
        self.values_until_closed(self.services.tables.subscribe_import_summary())
    }

    /// Every entry of Found's filter with how many of Found's rows it holds,
    /// live.
    pub fn subscribe_filter_entries(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<crate::PendingFilterEntry>> {
        self.values_until_closed(self.services.tables.subscribe_import_filter_entries())
    }

    /// What the rows at the caller's keys hold and the actions the person
    /// can run on them, whether or not a list shows them, as a live query
    /// whose keys the caller changes on the same query.
    pub fn subscribe_list_selection(
        &self,
    ) -> bae_store::live_query::CancellableLiveQuery<
        std::collections::BTreeSet<crate::CandidateKey>,
        crate::selection::SelectionSummary,
        bae_store::DbError,
    > {
        self.until_closed(self.services.tables.subscribe_import_selection())
    }

    /// `query` as a subscription a surface pulls the values of, which the
    /// library's closing ends.
    pub fn until_closed<Request, Projection>(
        &self,
        query: coven::ReconfigurableLiveQuery<Request, Projection, bae_store::DbError>,
    ) -> bae_store::live_query::CancellableLiveQuery<Request, Projection, bae_store::DbError>
    where
        Request: Clone + PartialEq + Send + Sync + 'static,
        Projection: Clone + PartialEq + Send + 'static,
    {
        bae_store::live_query::CancellableLiveQuery::new(
            query,
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// `query`'s values for a surface to pull, which the library's closing
    /// ends.
    pub fn values_until_closed<T: Clone + PartialEq + Send + 'static>(
        &self,
        query: impl bae_store::live_query::IntoLiveSource<T>,
    ) -> bae_store::live_query::LiveValues<T> {
        bae_store::live_query::LiveValues::new(
            query,
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// The key of the item at `position` in the list `request` shows, or of
    /// its last item when it ends before it; `None` for a list with no items.
    pub async fn list_key_at(
        &self,
        request: crate::ImportListRequest,
        position: u64,
    ) -> Result<Option<bae_store::list_windows::OrderKey>, bae_store::DbError> {
        self.services.tables.import_key_at(request, position).await
    }

    /// The keys of every release the list `request` shows, whether or not a
    /// surface loaded it: what Select All selects.
    pub async fn list_keys(
        &self,
        request: crate::ImportListRequest,
    ) -> Result<Vec<crate::CandidateKey>, crate::ImportError> {
        Ok(self.services.tables.import_list_keys(request).await?)
    }

    /// The releases the list `request` shows from the release `from` to the
    /// release `to`, both included, in the list's order: what a Shift gesture
    /// selects.
    pub async fn list_keys_between(
        &self,
        request: crate::ImportListRequest,
        from: crate::CandidateKey,
        to: crate::CandidateKey,
    ) -> Result<Vec<crate::CandidateKey>, crate::ImportError> {
        Ok(self.services.tables.import_list_keys_between(request, from, to).await?)
    }

    /// The rows at `selected` the list `request` shows, in its order: what a
    /// bulk action runs over; when `acting` names a command, only the rows
    /// that offer it.
    pub async fn list_selected(
        &self,
        request: crate::ImportListRequest,
        selected: Vec<crate::CandidateKey>,
        acting: Option<crate::CandidateAction>,
    ) -> Result<crate::selection::SelectedRows, crate::ImportError> {
        Ok(self.services.tables.load_import_list_selected(request, selected, acting).await?)
    }

    /// What showing the list `after` in place of the list `before` comes
    /// to, for the keys of `selected`: where it opens and which of them
    /// leave the selection. Core decides it from the two requests: another
    /// tab, filter or search opens at the first selected row it shows and
    /// takes out the selected rows it doesn't show; another order or fold
    /// keeps the person's place, and a fold takes out the selected rows it
    /// hid.
    pub async fn list_view_change(
        &self,
        before: crate::ImportListRequest,
        after: crate::ImportListRequest,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<crate::ImportListViewChange, crate::ImportError> {
        Ok(self.services.tables.import_list_view_change(before, after, selected).await?)
    }

    /// What revealing the candidate at `candidate_key` from the list
    /// `request` shows: the request on the tab its placement puts it on, with
    /// the filters cleared and its group opened, and where it sits there.
    /// `None` when no tab lists such a candidate.
    pub async fn reveal_candidate(
        &self,
        request: crate::ImportListRequest,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<crate::ImportListReveal>, crate::ImportError> {
        Ok(self.services.tables.reveal_import_candidate(request, candidate_key).await?)
    }

    /// Where the candidate at `candidate_key` sits in what the list `request`
    /// shows; `None` when it does not show it.
    pub async fn candidate_position(
        &self,
        request: crate::ImportListRequest,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<u64>, crate::ImportError> {
        Ok(self.services.tables.import_candidate_position(request, candidate_key).await?)
    }

    /// The first candidate in the order `order`, every tab's rows unfiltered,
    /// whose identification is queued, running or having its answer written.
    pub async fn first_identifying_candidate(
        &self,
        order: crate::ImportListOrder,
    ) -> Result<Option<crate::CandidateKey>, crate::ImportError> {
        Ok(self.services.tables.first_identifying_import_candidate(order).await?)
    }

    /// Every item of the list `request` shows, once, for a caller with no
    /// list open.
    pub async fn load_list(
        &self,
        request: crate::ImportListRequest,
    ) -> Result<Vec<crate::ImportListItem>, crate::ImportError> {
        let window = bae_store::ListWindow::top(u64::MAX);
        let read = self.services.tables.read_import_window(request, window).await?;
        Ok(read.rows.into_iter().map(|(item, _key)| item).collect())
    }

    /// Each watched folder's scan status, with an alert for each problem not
    /// among the raised alerts the request names, which start as none: a
    /// value after each write that changes it, whether a scan stored it or
    /// the folder scan coordinator wrote what it keeps for the scans, and
    /// after each new request.
    pub fn subscribe_folder_scans(
        &self,
    ) -> coven::ReconfigurableLiveQuery<
        crate::RaisedScanAlerts,
        crate::FolderScanProgress,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_folder_scan_progress()
    }

    /// The folders bae is scanning, each with how many folders its scan has
    /// stored: the first value at once, then a value after each write that
    /// changes one. Closing the library ends the subscription.
    pub fn subscribe_scanning_folders(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<crate::ScanningFolder>> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_scanning_folders(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// How many entries each tab holds, for a test that waits on a scan: the
    /// first value at once, then a value after each write that moves one.
    /// Closing the library ends it. A screen reads each tab's count from its
    /// open import list's count instead of this stream.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_tab_counts(
        &self,
    ) -> bae_store::live_query::LiveValues<crate::TriageTabCounts> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_tab_counts(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// How far the identifications and imports in flight have got: the
    /// first value at once, then a value after each write that moves them.
    /// Closing the library ends it.
    pub fn subscribe_counts(&self) -> bae_store::live_query::LiveValues<crate::ImportCounts> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_import_counts(),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// The identify state `key`'s pane shows and what is in flight for it:
    /// the first value at once, then a value after each write that changes
    /// it. Closing the library ends it.
    pub fn subscribe_candidate_runtime(
        &self,
        key: &crate::CandidateKey,
    ) -> bae_store::live_query::LiveValues<crate::CandidateRuntimeView> {
        bae_store::live_query::LiveValues::new(
            self.services.tables.subscribe_candidate_runtime(key),
            self.services.closing.child_token(),
            self.bugs(),
        )
    }

    /// One candidate as the pane reads it, once, with its standing among
    /// the candidates.
    pub async fn load_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::ImportCandidateDetail>, crate::ImportError> {
        Ok(self
            .services
            .tables
            .load_import_candidate(key)
            .await?
            .map(crate::ImportCandidateDetailProjection::resolve))
    }

    /// The pane's live read of the candidate the request names; `None` for
    /// no key or a key naming no release the list lists. Setting a new key
    /// points the same query at another candidate.
    pub fn subscribe_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::ImportCandidateDetailProjection>,
        crate::ImportError,
    > {
        self.services.tables.subscribe_import_candidate(initial)
    }

    /// The live read of the folder that failed validation the request names;
    /// `None` for no key or a key naming no such folder. Setting a new key
    /// points the same query at another folder.
    pub fn subscribe_invalid_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::InvalidCandidateDetail>,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_invalid_candidate(initial)
    }

    /// The live read of the images the lightbox of a candidate's folder
    /// shows; see `ImportTables::subscribe_folder_images`.
    #[cfg(desktop)]
    pub fn subscribe_folder_images(
        &self,
        initial: Option<crate::FolderImagesRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::FolderImagesRequest>,
        Option<crate::FolderImages>,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_folder_images(initial)
    }

    /// The first tab counts `accept` admits, waiting through the query's
    /// values until one does. The counts land after the commit they reflect,
    /// so a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_tab_counts(
        &self,
        mut accept: impl FnMut(&crate::TriageTabCounts) -> bool,
    ) -> crate::TriageTabCounts {
        let counts = self.subscribe_tab_counts();
        loop {
            let counts = counts.next().await.expect("the tab counts query stays open");
            if accept(&counts) {
                return counts;
            }
        }
    }

    /// Everything the list `request` shows, with its count, summary, filter
    /// entries and what the rows at `selected` hold and offer, in one read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn read_list_for_test(
        &self,
        request: crate::ImportListRequest,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<crate::list::ImportListRead, bae_store::DbError> {
        self.services.tables.read_import_list_for_test(request, selected).await
    }

    /// The list `request` shows followed through one window over every item,
    /// for a test that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_whole_list(
        &self,
        request: impl Into<crate::ImportListRequest>,
    ) -> crate::list::FollowedImportList {
        self.follow_list(request, bae_store::ListWindow::top(u64::MAX))
    }

    /// The list `request` shows followed through `window`, for a test that
    /// watches what changes off the window.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn follow_list(
        &self,
        request: impl Into<crate::ImportListRequest>,
        window: bae_store::ListWindow,
    ) -> crate::list::FollowedImportList {
        let request = request.into();
        let windows = self.subscribe_list_window(request.clone(), window);
        crate::list::FollowedImportList::follow(request, windows)
    }

    /// Everything `list` shows once its window delivers its next value, with
    /// what the rows at `selected` hold and offer, read beside it in one
    /// snapshot.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn next_list_read(
        &self,
        list: &crate::list::FollowedImportList,
        selected: Vec<crate::CandidateKey>,
    ) -> crate::list::ImportListRead {
        list.changed().await;
        self.read_list_for_test(list.request().clone(), selected).await.expect("the list reads")
    }

    /// The list `request` shows, read through every row, once `accept`
    /// admits it. The query delivers the new list after the commit it
    /// reflects, so a test that just observed a scan event waits here for the
    /// read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        request: impl Into<crate::ImportListRequest>,
        mut accept: impl FnMut(&crate::list::ImportListRead) -> bool,
    ) -> crate::list::ImportListRead {
        let list = self.subscribe_whole_list(request);
        loop {
            let read = self.next_list_read(&list, Vec::new()).await;
            if accept(&read) {
                return read;
            }
        }
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
