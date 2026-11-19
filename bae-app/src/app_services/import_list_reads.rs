//! The import list's live reads on [`AppServices`], in the shapes every list
//! uses: the reads a request opens (its window and a key's position) and its
//! count, then the summary, Found's filter entries and the selection. Each read
//! takes the request the surface shows, and ends when the library closes.

use super::*;
use bae_import::ImportListRequest;
use bae_store::live_query::LiveValues;

impl AppServices {
    /// The reads the list `request` shows opens: its window, which sends the
    /// window's rows at once, again after each write the window reads, and
    /// again each time the view moves the window; the position of a key; and
    /// the key at a position.
    pub fn import_list_reads(&self, request: ImportListRequest) -> bae_views::list_view::ListReads {
        let (for_position, for_key) = (request.clone(), request.clone());
        super::list_reads::list_reads(
            self,
            move |services, window| services.subscribe_import_list_window(request.clone(), window),
            move |services, key| services.subscribe_import_list_position(for_position.clone(), key),
            move |services, position| {
                let request = for_key.clone();
                async move { services.inner.import.list_key_at(request, position).await }
            },
        )
    }

    /// One window of the list `request` shows, each item with its key.
    fn subscribe_import_list_window(
        &self,
        request: ImportListRequest,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        bae_views::LiveListWindow::new(
            self.inner.import.subscribe_list_window(request, window),
            |_read, read: bae_import::ImportWindow| {
                let (items, keys) = read.rows.into_iter().unzip();
                Some(bae_views::ListWindowValue {
                    window: read.window,
                    rows: bae_views::ListRows::Import(items),
                    keys,
                })
            },
        )
    }

    /// How many items the list `request` shows holds, and how many its
    /// narrowing keeps: the first value at once, then one each time a write
    /// moves either.
    pub fn import_list_count(
        &self,
        request: ImportListRequest,
    ) -> LiveValues<bae_views::ListCount> {
        self.inner.import.values_until_closed(
            self.inner.import.subscribe_list_count(request).process(|count| {
                Ok(bae_views::ListCount {
                    rows: count.rows,
                    items: count.rows,
                    extra: bae_views::ListCountExtra::Import {
                        narrowed: count.narrowed,
                        tabs: count.tabs,
                    },
                })
            }),
        )
    }

    /// How many items of the list `request` shows come before the one at
    /// `key`.
    fn subscribe_import_list_position(
        &self,
        request: ImportListRequest,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        bae_views::LiveListPosition::new(
            self.inner.import.subscribe_list_position(request, key),
            |read, rows_before| {
                Some(bae_views::ListPosition { key: read.request.clone(), rows_before })
            },
        )
    }

    /// Whether bae watches any folder and whether Found has groups.
    pub fn import_list_summary(&self) -> LiveValues<bae_import::ImportQueueSummary> {
        self.inner.import.subscribe_list_summary()
    }

    /// Every entry of Found's filter with how many of Found's rows it holds.
    pub fn import_list_filter_entries(&self) -> LiveValues<Vec<bae_import::PendingFilterEntry>> {
        self.inner.import.subscribe_filter_entries()
    }

    /// What the rows the surface keeps selected hold and the actions the
    /// person can run on them, whatever the list shows. The selection starts
    /// empty, and the caller changes it on this same read.
    pub fn import_list_selection(&self) -> crate::live_reads::LiveImportSelection {
        crate::live_reads::LiveImportSelection::new(
            self.inner.import.subscribe_list_selection(),
            |_read, summary| Some(summary),
        )
    }

    delegate_async!(import, import_list_keys => list_keys(request: ImportListRequest) -> Result<Vec<bae_import::CandidateKey>, bae_import::ImportError>);
    delegate_async!(import, import_list_keys_between => list_keys_between(request: ImportListRequest, from: bae_import::CandidateKey, to: bae_import::CandidateKey) -> Result<Vec<bae_import::CandidateKey>, bae_import::ImportError>);
    delegate_async!(import, import_list_selected => list_selected(request: ImportListRequest, selected: Vec<bae_import::CandidateKey>, acting: Option<bae_import::CandidateAction>) -> Result<bae_import::selection::SelectedRows, bae_import::ImportError>);
    delegate_async!(import, import_list_view_change => list_view_change(before: ImportListRequest, after: ImportListRequest, selected: Vec<bae_import::CandidateKey>) -> Result<bae_import::ImportListViewChange, bae_import::ImportError>);
    delegate_async!(import, import_reveal_candidate => reveal_candidate(request: ImportListRequest, candidate_key: &bae_import::CandidateKey) -> Result<Option<bae_import::ImportListReveal>, bae_import::ImportError>);
    delegate_async!(import, import_candidate_position => candidate_position(request: ImportListRequest, candidate_key: &bae_import::CandidateKey) -> Result<Option<u64>, bae_import::ImportError>);
    delegate_async!(import, import_first_identifying_candidate => first_identifying_candidate(order: bae_import::ImportListOrder) -> Result<Option<bae_import::CandidateKey>, bae_import::ImportError>);
    delegate_async!(import, load_import_items => load_list(request: ImportListRequest) -> Result<Vec<bae_import::ImportListItem>, bae_import::ImportError>);

    delegate_sync!(#[cfg(any(test, feature = "test-utils"))] import, import_list_window_for_test => subscribe_list_window(request: ImportListRequest, initial: bae_store::ListWindow) -> bae_store::live_query::CancellableLiveQuery<bae_store::ListWindow, bae_import::ImportWindow, bae_store::DbError>);
    delegate_sync!(#[cfg(any(test, feature = "test-utils"))] import, import_list_count_for_test => subscribe_list_count(request: ImportListRequest) -> coven::LiveQuery<bae_import::ImportListCount, bae_store::DbError>);
    delegate_async!(#[cfg(any(test, feature = "test-utils"))] import, import_list_key_at_for_test => list_key_at(request: ImportListRequest, position: u64) -> Result<Option<bae_store::list_windows::OrderKey>, bae_store::DbError>);
    delegate_async!(#[cfg(any(test, feature = "test-utils"))] import, read_import_list_for_test => read_list_for_test(request: ImportListRequest, selected: Vec<bae_import::CandidateKey>) -> Result<bae_import::ImportListRead, bae_store::DbError>);
    delegate_sync!(#[cfg(any(test, feature = "test-utils"))] import, follow_import_list_for_test => follow_list(request: ImportListRequest, window: bae_store::ListWindow) -> bae_import::FollowedImportList);
}
