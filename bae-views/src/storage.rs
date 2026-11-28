//! The Storage Manager's view over releases: the paged list with its
//! counts and totals.
//!
//! Its tables and the triggers keeping them are `storage.sql`.

use super::*;
#[cfg(any(test, feature = "test-utils"))]
use crate::album_detail::StoragePage;
use crate::album_detail::StorageRow;
use bae_store::statement_rows;
pub(crate) use order::{StorageList, StorageScope};
use reads::*;
use row::StorageRowParts;

mod found_reads;
mod kept_window_reads;
mod key_reads;
mod lists;
mod order;
pub mod order_fragments;
mod position_reads;
mod reads;
mod row;
mod top_reads;
mod window_reads;

#[cfg(test)]
pub(crate) use lists::storage_rows_before_on;
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use lists::storage_window_on;
pub(crate) use lists::{
    storage_key_at_on, storage_rows_before_read, storage_window_of, storage_window_read,
};

impl LibraryViews {
    /// Count storage rows matching `filter`: the set `get_storage_page` pages.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let scope = StorageScope::new(filter, "");
        self.store.read(move |sql| Ok(storage_counts_on(&sql, &scope)?.0)).await
    }

    /// The size of every file of the storage rows matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let scope = StorageScope::new(filter, "");
        self.store.read(move |sql| Ok(storage_counts_on(&sql, &scope)?.1)).await
    }

    /// One page of the Storage Manager list, pre-sorted and pre-filtered.
    /// `total_count` counts the filtered subset, not the whole library.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, DbError> {
        let raw_rows = self.get_db_storage_page(sort, filter, offset, limit).await?;
        let total_count = self.get_storage_count(filter).await?;
        let rows = raw_rows.into_iter().map(StorageRow::from_raw).collect();
        Ok(StoragePage { rows, total_count })
    }

    /// One page of the Storage Manager list as its rows read it,
    /// unresolved: the window from the key of the row at `offset`.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn get_db_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let list = StorageList::new(sort, filter, "");
        self.store
            .read(move |sql| {
                if offset >= storage_counts_on(&sql, &list.scope)?.0 {
                    return Ok(Vec::new());
                }
                let from = storage_key_at_on(&sql, &list, offset)?;
                let window = storage_window_on(&sql, &list, &ListWindow { from, limit })?;
                Ok(window.rows.into_iter().map(|(row, _)| row).collect::<Vec<_>>())
            })
            .process(process_storage_rows)
            .await
    }

    /// Follow one window of the Storage Manager list under `sort`, `filter`
    /// and the search text `search`: the rows from a key of its order on.
    pub fn subscribe_storage_window(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbStorageRow>, bae_store::DbError>
    {
        let list = StorageList::new(sort, filter, search);
        self.store
            .subscribe_reconfigurable(initial, move |window| storage_window_read(&list, window))
            .process(|window, rows| {
                storage_window_of(window, rows)?.try_map_rows(StorageRowParts::process)
            })
    }

    /// Follow how many releases the Storage Manager list holds under `filter`
    /// and `search`, and their files' total size. This count is a query of
    /// its own, apart from the windows' rows, because every release added or
    /// removed changes it.
    pub fn subscribe_storage_count(
        &self,
        filter: StorageFilter,
        search: &str,
    ) -> coven::LiveQuery<StorageCounts, bae_store::DbError> {
        let scope = StorageScope::new(filter, search);
        let searching = matches!(scope, StorageScope::Found { .. });
        self.store.subscribe(storage_counts_read(&scope)).process(move |rows| {
            let (rows, total_size) = crate::storage_totals::storage_totals_of(rows)?;
            Ok(StorageCounts { rows, total_size, searching })
        })
    }

    /// Follow how many releases of the Storage Manager list come before the
    /// row at a key. The caller sets a new key on the same live query.
    pub fn subscribe_storage_position(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let list = StorageList::new(sort, filter, search);
        self.store
            .subscribe_reconfigurable(initial, move |key| storage_rows_before_read(&list, key))
            .process(|_, before| bae_store::exactly_one::<coven::ReadAs<u64>, _>(before))
    }

    /// The releases the Storage Manager list under `sort`, `filter` and the
    /// `search` text shows from the release `from` to the release `to`, both
    /// included, in the list's order: what a Shift-click selects. The one
    /// the list holds when it holds only one of the two, and none when it
    /// holds neither.
    pub async fn storage_ids_between(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, DbError> {
        let list = StorageList::new(sort, filter, search);
        let (from, to) = (from.to_string(), to.to_string());
        self.store.read(move |sql| storage_ids_between_on(&sql, &list, from, to)).await
    }

    /// Every release the Storage Manager list under `filter` and the `search`
    /// text shows: what Select All selects.
    pub async fn storage_ids(
        &self,
        filter: StorageFilter,
        search: &str,
    ) -> Result<Vec<String>, DbError> {
        let scope = StorageScope::new(filter, search);
        self.store.read(move |sql| storage_ids_on(&sql, &scope)).await
    }

    /// The key of the release at `position` of the Storage Manager list, or
    /// of its last row when it ends before it; `None` for an empty list.
    pub async fn storage_key_at(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let list = StorageList::new(sort, filter, search);
        self.store.read(move |sql| storage_key_at_on(&sql, &list, position)).await
    }

    /// Follow the rows of the releases `initial` names that the Storage
    /// Manager list under `filter` and `search` holds, wherever in it they
    /// sit, with the storage actions every one of them allows. A selected
    /// release the list no longer holds is absent, so the inspector never
    /// shows, and the storage actions never act on, a release the list
    /// doesn't. The caller changes the selected ids through the request
    /// handle, and the same query reads them again.
    pub fn subscribe_storage_selection(
        &self,
        filter: StorageFilter,
        search: &str,
        initial: BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<BTreeSet<String>, DbStorageSelection, bae_store::DbError>
    {
        let scope = StorageScope::new(filter, search);
        self.store
            .subscribe_reconfigurable(initial, move |selected| {
                storage_selection_read(&scope, selected)
            })
            .process(|_, found| storage_selection_of(found))
    }

    /// Follow the transfer queues holding work, as the Storage Manager shows
    /// them.
    pub fn subscribe_storage_queues(
        &self,
    ) -> coven::LiveQuery<crate::StorageQueues, bae_store::DbError> {
        self.store.subscribe_row(StorageQueueWorkParams {}).process(storage_queues_of)
    }
}

/// What the Storage Manager's selection read returns: the rows of the selected
/// releases the list holds, what all of them allow, and the one release the
/// list holds among them when it holds exactly one.
pub type DbStorageSelection =
    (Vec<DbStorageRow>, crate::DbStorageActionFlags, Option<crate::StorageSoleRelease>);

/// How many releases the Storage Manager list holds, their files' total size,
/// and whether a search narrows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCounts {
    pub rows: u64,
    pub total_size: u64,
    /// Whether the search text narrows the list: it is a library search, not
    /// blank.
    pub searching: bool,
}

/// The Storage Manager list's count as the list shows it: its releases,
/// their files' total size, and whether a search narrows it.
pub fn resolve_storage_count(counts: StorageCounts) -> crate::ListCount {
    crate::ListCount {
        rows: counts.rows,
        items: counts.rows,
        extra: crate::ListCountExtra::Storage {
            total_size: counts.total_size,
            searching: counts.searching,
        },
    }
}

/// One delivery of a Storage Manager window as the list shows it.
pub fn resolve_storage_window(read: WindowRead<DbStorageRow>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, |rows| {
        crate::ListRows::Storage(rows.into_iter().map(StorageRow::from_raw).collect())
    })
}

/// The rows of the selected releases the Storage Manager list holds, the
/// storage actions every one of them allows, and whether Cancel is the only
/// action one of them offers.
pub fn resolve_storage_selection(
    asked: &BTreeSet<String>,
    (rows, actions, sole): DbStorageSelection,
) -> crate::StorageSelection {
    crate::StorageSelection {
        asked: asked.iter().cloned().collect(),
        sole,
        selected: rows.into_iter().map(StorageRow::from_raw).collect(),
        selection_actions: actions.list(),
        cancel_only: actions.cancel_only,
    }
}

/// The transfer queues holding files waiting or on the move, each with what
/// holds it back, and for the pin and output queues the release running. A
/// queue holding only failed entries holds no work.
fn storage_queues_of(work: StorageQueueWorkRow) -> Result<crate::StorageQueues, DbError> {
    let uploads =
        if work.uploading { Some(crate::UploadQueueRun { wait: work.held.wait()? }) } else { None };
    let run = |(wait, active)| crate::ReleaseQueueRun { wait, active };
    Ok(crate::StorageQueues {
        uploads,
        downloads: work.downloads.into_run().map(run),
        outputs: work.outputs.into_run().map(run),
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn process_storage_rows(rows: Vec<StorageRowParts>) -> Result<Vec<DbStorageRow>, DbError> {
    rows.into_iter().map(StorageRowParts::process).collect()
}

/// The read of how many releases the list of `scope` holds and their files'
/// total size: the stored totals (`storage_totals.sql`), and for a list a
/// search narrows, a count over the releases the search finds.
fn storage_counts_read(scope: &StorageScope) -> coven::ReadAs<crate::storage_totals::StoredTotals> {
    match scope {
        StorageScope::All => crate::storage_totals::storage_totals_read(None),
        StorageScope::Kept(stored) => crate::storage_totals::storage_totals_read(Some(*stored)),
        StorageScope::Found { pattern, stored } => {
            let statement =
                found_reads::FoundTotalsParams { search: Some(pattern.clone()), stored: *stored };
            coven::ReadAs::new(statement, |found| Some((found.found, found.total_size)))
        }
    }
}

/// How many releases the list of `scope` holds and their files' total size.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn storage_counts_on(
    sql: &SqlReadContext<'_>,
    scope: &StorageScope,
) -> Result<(u64, u64), DbError> {
    crate::storage_totals::storage_totals_of(sql.reads(&storage_counts_read(scope))?)
}

/// The read of the rows of the `release_ids` the list of `scope` holds,
/// wherever in it they sit.
fn storage_selection_read(
    scope: &StorageScope,
    release_ids: &BTreeSet<String>,
) -> Result<SelectedStorageRowsParams, DbError> {
    let selected = serde_json::to_string(release_ids)
        .map_err(|error| DbError::encode("selected release ids", error))?;
    let (stored, search) = scope.bound();
    Ok(SelectedStorageRowsParams { selected, stored, search })
}

/// The rows of the selected releases the list holds, by release id; what
/// every one of those rows allows; and the one release among them, with its
/// album's title, when the list holds exactly one.
fn storage_selection_of(found: Vec<SelectedStorageRowsRow>) -> Result<DbStorageSelection, DbError> {
    // Every row holds the same aggregates over all of them.
    let actions = found.first().map_or_else(crate::DbStorageActionFlags::none, |row| {
        crate::DbStorageActionFlags {
            upload: row.every_can_upload,
            pin: row.every_can_pin,
            unpin: row.every_can_unpin,
            move_to_this_device: row.every_can_move_to_this_device,
            cancel_only: row.any_cancel_only,
        }
    });
    let sole = found.first().filter(|row| row.sole).map(|row| crate::StorageSoleRelease {
        release_id: row.shown.row_release_id.clone(),
        album_title: row.shown.album.title().to_string(),
    });
    let rows =
        found.into_iter().map(|row| row.shown.into_parts()?.process()).collect::<Result<_, _>>()?;
    Ok((rows, actions, sole))
}

/// The releases `list` shows from the release `from` to the release `to`,
/// both included, in its order.
fn storage_ids_between_on(
    sql: &SqlReadContext<'_>,
    list: &StorageList,
    from: String,
    to: String,
) -> Result<Vec<String>, DbError> {
    let (stored, search) = list.scope.bound();
    let statement =
        StorageBetweenParams { sort: list.lead.code().to_string(), stored, search, from, to };
    Ok(statement_rows(sql, &statement)?.into_iter().map(|row| row.release_id).collect())
}

/// Every release the list of `scope` shows.
fn storage_ids_on(sql: &SqlReadContext<'_>, scope: &StorageScope) -> Result<Vec<String>, DbError> {
    let (stored, search) = scope.bound();
    Ok(statement_rows(sql, &StorageIdsParams { stored, search })?
        .into_iter()
        .map(|row| row.release_id)
        .collect())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
