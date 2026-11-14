//! The Storage Manager's list as the integration tests read it: a window of
//! its rows, its count and its selection, each a live read of its own.

use bae_library::LibraryManager;
use bae_store::live_query::LiveValues;
use bae_store::ListWindow;
use bae_views::album_detail::StorageRow;
use bae_views::{
    LibraryViews, ListCount, ListRows, ListWindowValue, LiveListWindow, StorageFilter,
    StorageSortCriterion, StorageSortField,
};

/// The list ordered by album title, ascending.
pub(crate) fn by_title() -> StorageSortCriterion {
    StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: bae_store::SortDirection::Ascending,
    }
}

/// The window of up to `limit` rows from the top of the list under `sort` and
/// `filter`, nothing searched.
pub(crate) fn storage_top(
    manager: &LibraryManager,
    views: &LibraryViews,
    sort: &StorageSortCriterion,
    filter: StorageFilter,
    limit: u64,
) -> LiveListWindow {
    storage_window(
        manager,
        views,
        sort,
        filter,
        ListWindow::Forward { from: None, until: None, limit },
    )
}

/// `window` of the list under `sort` and `filter`, nothing searched.
pub(crate) fn storage_window(
    manager: &LibraryManager,
    views: &LibraryViews,
    sort: &StorageSortCriterion,
    filter: StorageFilter,
    window: ListWindow,
) -> LiveListWindow {
    LiveListWindow::new(
        manager.until_closed(views.subscribe_storage_window(sort, filter, "", window)),
        |_, read| Some(bae_views::storage::resolve_storage_window(read)),
    )
}

/// How many rows the list under `filter` holds, and their total size.
pub(crate) fn storage_count(
    manager: &LibraryManager,
    views: &LibraryViews,
    filter: StorageFilter,
) -> LiveValues<ListCount> {
    manager.values_until_closed(views.subscribe_storage_count(filter, "").process(|counts| {
        Ok(ListCount {
            rows: counts.rows,
            extra: bae_views::ListCountExtra::Storage {
                total_size: counts.total_size,
                searching: counts.searching,
            },
        })
    }))
}

/// The rows a delivered window holds.
pub(crate) fn storage_rows(value: &ListWindowValue) -> &[StorageRow] {
    match &value.rows {
        ListRows::Storage(rows) => rows,
        _ => panic!("the Storage Manager's window holds its rows"),
    }
}

/// The selection of the list under `filter`, nothing searched, holding none
/// until it is set.
pub(crate) fn storage_selection(
    manager: &LibraryManager,
    views: &LibraryViews,
    filter: StorageFilter,
) -> bae_app::live_reads::LiveStorageSelection {
    bae_app::live_reads::LiveStorageSelection::new(
        manager.until_closed(views.subscribe_storage_selection(filter, "", Default::default())),
        |_, projection| Some(bae_views::storage::resolve_storage_selection(projection)),
    )
}
