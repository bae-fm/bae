//! The open import list's reads as the lists' pull objects take them: each
//! window, its count and the position of a key are live reads of their own,
//! the same shapes as every other list's, built over the list's own
//! cancellation, and the chrome around it, Found's filter entries and the
//! selection beside them.

use bae_import::LiveImportList;
use bae_store::live_query::LiveValues;

/// One window of `list`, as its first value at once, then one for each write
/// inside its range and one for each window it is moved to.
pub fn import_list_window(
    list: &LiveImportList,
    window: bae_store::ListWindow,
) -> bae_views::LiveListWindow {
    bae_views::LiveListWindow::new(
        list.until_closed(list.subscribe_window(window)),
        |_read, window: bae_import::ImportWindow| {
            let bae_import::ImportWindow { read, leading_header } = window;
            Some(bae_views::ListWindowValue {
                window: read.window,
                rows: bae_views::ListRows::Import { items: read.rows, leading_header },
                first: read.first,
                beyond: read.beyond,
            })
        },
    )
}

/// How many items `list` holds, and how many its narrowing keeps: the first
/// value at once, then one each time a write moves either.
pub fn import_list_count(list: &LiveImportList) -> LiveValues<bae_views::ListCount> {
    list.values_until_closed(list.subscribe_count().process(|count| {
        Ok(bae_views::ListCount {
            rows: count.rows,
            extra: bae_views::ListCountExtra::Import { narrowed: count.narrowed },
        })
    }))
}

/// How many items of `list` come before the one at `key`.
pub fn import_list_position(
    list: &LiveImportList,
    key: bae_store::list_windows::OrderKey,
) -> bae_views::LiveListPosition {
    bae_views::LiveListPosition::new(
        list.until_closed(list.subscribe_position(key)),
        |read, rows_before| {
            Some(bae_views::ListPosition { key: read.request.clone(), rows_before })
        },
    )
}

/// What the chrome around `list` shows.
pub fn import_list_summary(list: &LiveImportList) -> LiveValues<bae_import::ImportQueueSummary> {
    list.values_until_closed(list.subscribe_summary())
}

/// Every entry of Found's filter with how many of Found's rows it holds.
pub fn import_list_filter_entries(
    list: &LiveImportList,
) -> LiveValues<Vec<bae_import::PendingFilterEntry>> {
    list.values_until_closed(list.subscribe_filter_entries())
}

/// What `list`'s selection holds and can be told to do.
pub fn import_list_selection(
    list: &LiveImportList,
) -> LiveValues<bae_import::selection::SelectionSummary> {
    list.values_until_closed(list.subscribe_selection())
}
