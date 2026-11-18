//! The reads of a list a surface follows, each its own live query over the
//! surface's request: a window (a range of the list's order, read by its
//! keys), how many items the list holds, where a key sits in it, the watched
//! folders, whether Found has groups, Found's filter entries, and what the
//! selected rows hold and which actions they offer. coven reruns each when a
//! write changes what it read, so when a scan writes rows, coven reruns the
//! count and the windows whose ranges hold those rows, and nothing else.
//! Beside them, the reads a surface makes once: the key at a position.

use super::*;
use crate::list::{ImportListCount, ImportQueueSummary, ImportWindow};
use bae_store::list_windows::OrderKey;
use bae_store::ListWindow;

/// `window` of the list `request` shows: the items of its range, and the
/// header of the group they start inside of.
fn window_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
    window: &ListWindow,
) -> Result<ImportWindow, DbError> {
    window::read_window(sql, request, window)
}

/// How many items sit before the one at `key` in the list `request` shows.
fn position_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
    key: &OrderKey,
) -> Result<u64, DbError> {
    Items::new(request.clone()).rows_before(sql, key)
}

impl ImportTables {
    /// One window of the list `request` shows as a live query whose request
    /// is the window, so moving its bounds reruns it on the same
    /// subscription. It reads its own range of the list's order, so a write
    /// reruns it only when the write is in that range.
    pub(crate) fn subscribe_import_window(
        &self,
        request: ImportListRequest,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, ImportWindow, DbError> {
        self.store
            .subscribe_reconfigurable(initial, move |window, sql| window_on(&sql, &request, window))
    }

    /// How many items the list `request` shows holds and how many its
    /// narrowing keeps, live.
    pub(crate) fn subscribe_import_count(
        &self,
        request: ImportListRequest,
    ) -> coven::LiveQuery<ImportListCount, DbError> {
        self.store.subscribe(move |sql| summary::count(&sql, &request))
    }

    /// How many items of the list `request` shows sit before the one at a
    /// key, as a live query whose key the caller changes on the same query.
    pub(crate) fn subscribe_import_position(
        &self,
        request: ImportListRequest,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, DbError> {
        self.store
            .subscribe_reconfigurable(initial, move |key, sql| position_on(&sql, &request, key))
    }

    /// The watched folders and whether Found has groups, live.
    pub(crate) fn subscribe_import_summary(&self) -> coven::LiveQuery<ImportQueueSummary, DbError> {
        self.store.subscribe(move |sql| summary::summary(&sql))
    }

    /// Every entry of Found's filter with how many of Found's rows it holds,
    /// live.
    pub(crate) fn subscribe_import_filter_entries(
        &self,
    ) -> coven::LiveQuery<Vec<crate::PendingFilterEntry>, DbError> {
        self.store.subscribe(move |sql| summary::filter_entries(&sql))
    }

    /// What the rows at the caller's keys hold and the actions the person
    /// can run on them, in any list and whether or not a list shows them, as
    /// a live query whose keys the caller changes on the same query.
    pub(crate) fn subscribe_import_selection(
        &self,
    ) -> coven::ReconfigurableLiveQuery<
        std::collections::BTreeSet<crate::CandidateKey>,
        crate::selection::SelectionSummary,
        DbError,
    > {
        self.store.subscribe_reconfigurable(Default::default(), move |keys, sql| {
            selection::selection(&sql, &keys.iter().cloned().collect::<Vec<_>>())
        })
    }

    /// One window of the list `request` shows, once.
    pub async fn read_import_window(
        &self,
        request: ImportListRequest,
        window: ListWindow,
    ) -> Result<ImportWindow, DbError> {
        self.store.read(move |sql| window_on(&sql, &request, &window)).await
    }

    /// The key of the item at `position` in the list `request` shows, or of
    /// its last item when it ends before it; `None` for a list with no items.
    /// A jump reads this once and starts a chain of windows there.
    pub async fn import_key_at(
        &self,
        request: ImportListRequest,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        self.store.read(move |sql| Items::new(request).key_at(&sql, position)).await
    }

    /// How many items of the list `request` shows sit before the one at
    /// `key`.
    #[cfg(test)]
    pub(super) async fn import_rows_before_for_test(
        &self,
        request: ImportListRequest,
        key: OrderKey,
    ) -> Result<u64, DbError> {
        self.store.read(move |sql| position_on(&sql, &request, &key)).await
    }

    /// Every item of the list `request` shows, with its count, summary,
    /// filter entries and what the rows at `selected` hold and offer, read in
    /// one snapshot.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn read_import_list_for_test(
        &self,
        request: ImportListRequest,
        selected: Vec<crate::CandidateKey>,
    ) -> Result<crate::list::ImportListRead, DbError> {
        self.store
            .read(move |sql| {
                let window = ListWindow::top(u64::MAX);
                let rows = window_on(&sql, &request, &window)?;
                Ok((
                    rows,
                    summary::count(&sql, &request)?,
                    summary::summary(&sql)?,
                    summary::filter_entries(&sql)?,
                    selection::selection(&sql, &selected)?,
                ))
            })
            .process(|(rows, count, summary, entries, selection)| {
                Ok(crate::list::ImportListRead::of(rows, count, summary, entries, selection))
            })
            .await
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
