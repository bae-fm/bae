//! The reads of a list a surface follows, each its own live query: a window
//! (a range of the list's order, read by its keys), how many items the list
//! holds, where a key sits in it, the chrome around it, Found's filter
//! entries, and what the selection holds and offers. Each reruns when a
//! write changes what it read, so a scan's rows rerun the count and the
//! windows they land in, and nothing else. Beside them, the reads a surface
//! makes once: the key at a position.

use super::order::{Item, ItemKey, WindowItems};
use super::summary::closed;
use super::*;
use crate::list::{ImportListCount, ImportListItem, ImportQueueSummary, ImportWindow};
use bae_store::list_windows::OrderKey;
use bae_store::{ListWindow, WindowRead};

/// One window of a list as one read found it, its rows not yet put together.
#[derive(Clone, PartialEq)]
struct WindowRows {
    window: ListWindow,
    rows: Vec<window::WindowItemRows>,
    leading_header: Option<window::WindowItemRows>,
    first: Option<OrderKey>,
    beyond: Option<OrderKey>,
}

/// The header of the group the first of `keys` is a member of, as an item
/// key, when the window starts inside the group: its header leads its
/// members, so it is not in the window.
fn leading_header(keys: &[ItemKey]) -> Option<ItemKey> {
    let first = keys.first()?;
    let group = first.group_folder.as_ref().filter(|_| first.item != Item::Group)?;
    Some(ItemKey {
        watched_folder_path: first.watched_folder_path.clone(),
        item: Item::Group,
        key: group.clone(),
        group_folder: Some(group.clone()),
    })
}

/// `window` of the list `list` shows: the items of its range, and the header
/// of the group they start inside of.
fn window_on(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    window: &ListWindow,
) -> Result<WindowRows, DbError> {
    let row = list_row(sql, list)?.ok_or_else(|| closed(list))?;
    let WindowItems { mut keys, first, beyond } = row.items(Shown::View).range(sql, window)?;
    let leading = leading_header(&keys);
    if let Some(header) = &leading {
        keys.insert(0, header.clone());
    }
    let mut rows = window::materialise(sql, &row, &keys)?;
    let leading_header = leading.map(|_| rows.remove(0));
    Ok(WindowRows { window: window.clone(), rows, leading_header, first, beyond })
}

impl WindowRows {
    fn process(self) -> Result<ImportWindow, DbError> {
        let rows = self
            .rows
            .into_iter()
            .map(window::WindowItemRows::process)
            .collect::<Result<Vec<ImportListItem>, DbError>>()?;
        Ok(ImportWindow {
            read: WindowRead { window: self.window, rows, first: self.first, beyond: self.beyond },
            leading_header: self.leading_header.map(window::WindowItemRows::process).transpose()?,
        })
    }
}

/// How many items sit before the one at `key` in `list`.
fn position_on(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    key: &OrderKey,
) -> Result<u64, DbError> {
    let row = list_row(sql, list)?.ok_or_else(|| closed(list))?;
    row.items(Shown::View).rows_before(sql, key)
}

impl ImportTables {
    /// One window of `list` as a live query whose request is the window, so
    /// moving its bounds reruns it on the same subscription. It reads its own
    /// range of the list's order, so a write reruns it only when the write
    /// is in that range.
    pub(crate) fn subscribe_import_window(
        &self,
        list: ImportListId,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, ImportWindow, DbError> {
        self.store
            .subscribe_reconfigurable(initial, move |window, sql| window_on(&sql, list, window))
            .process(|rows| rows.process())
    }

    /// How many items `list` holds and how many its narrowing keeps, live.
    pub(crate) fn subscribe_import_count(
        &self,
        list: ImportListId,
    ) -> coven::LiveQuery<ImportListCount, DbError> {
        self.store.subscribe(move |sql| summary::count(&sql, list))
    }

    /// How many items of `list` sit before the one at a key, as a live query
    /// whose key moves in place.
    pub(crate) fn subscribe_import_position(
        &self,
        list: ImportListId,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, DbError> {
        self.store.subscribe_reconfigurable(initial, move |key, sql| position_on(&sql, list, key))
    }

    /// What the chrome around `list` shows, live.
    pub(crate) fn subscribe_import_summary(
        &self,
        list: ImportListId,
    ) -> coven::LiveQuery<ImportQueueSummary, DbError> {
        self.store.subscribe(move |sql| summary::summary(&sql, list))
    }

    /// Every entry of Found's filter with how many of Found's rows it holds,
    /// live.
    pub(crate) fn subscribe_import_filter_entries(
        &self,
    ) -> coven::LiveQuery<Vec<crate::PendingFilterEntry>, DbError> {
        self.store.subscribe(move |sql| summary::filter_entries(&sql))
    }

    /// What `list`'s selection holds and can be told to do, live.
    pub(crate) fn subscribe_import_selection(
        &self,
        list: ImportListId,
    ) -> coven::LiveQuery<crate::selection::SelectionSummary, DbError> {
        self.store.subscribe(move |sql| selection::selection(&sql, list))
    }

    /// One window of `list`, once.
    pub async fn read_import_window(
        &self,
        list: ImportListId,
        window: ListWindow,
    ) -> Result<ImportWindow, DbError> {
        self.store
            .read(move |sql| window_on(&sql, list, &window))
            .process(|rows| rows.process())
            .await
    }

    /// The key of the item at `position` in `list`, or of its last item when
    /// it ends before it; `None` for a list with no items. A jump reads this
    /// once and starts a chain of windows there.
    pub async fn import_key_at(
        &self,
        list: ImportListId,
        position: u64,
    ) -> Result<Option<OrderKey>, ImportListError> {
        self.store
            .read(move |sql| {
                let Some(row) = list_row(&sql, list)? else {
                    return Ok(None);
                };
                let tabs = summary::tab_counts_on(&sql)?;
                let listed = summary::listed(&sql, &row, &tabs)?;
                Ok(Some(row.items(listed.shown(position)).key_at(&sql, position)?))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// Every item of `list`, with its count, summary, filter entries and
    /// selection, read in one snapshot.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn read_import_list_for_test(
        &self,
        list: ImportListId,
    ) -> Result<crate::list::ImportListRead, DbError> {
        self.store
            .read(move |sql| {
                let window = ListWindow::Forward { from: None, until: None, limit: u64::MAX };
                let rows = window_on(&sql, list, &window)?;
                Ok((
                    rows,
                    summary::count(&sql, list)?,
                    summary::summary(&sql, list)?,
                    summary::filter_entries(&sql)?,
                    selection::selection(&sql, list)?,
                ))
            })
            .process(|(rows, count, summary, entries, selection)| {
                Ok(crate::list::ImportListRead::of(
                    rows.process()?.read,
                    count,
                    summary,
                    entries,
                    selection,
                ))
            })
            .await
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
