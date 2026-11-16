//! This file gives the open import list's live reads in the shapes every
//! list uses: each window, the count and a key's position, then the summary,
//! Found's filter entries and the selection. Each read ends when the list
//! closes.

use bae_import::LiveImportList;
use bae_store::live_query::LiveValues;

/// One window of `list`: the stream sends the window's rows at once, again
/// after each write inside the window, and again each time the view moves
/// the window.
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
            items: count.rows,
            extra: bae_views::ListCountExtra::Import { narrowed: count.narrowed, tabs: count.tabs },
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

/// The watched folders, whether Found has groups, and what the view narrows
/// by, for `list`.
pub fn import_list_summary(list: &LiveImportList) -> LiveValues<bae_import::ImportQueueSummary> {
    list.values_until_closed(list.subscribe_summary())
}

/// Every entry of Found's filter with how many of Found's rows it holds.
pub fn import_list_filter_entries(
    list: &LiveImportList,
) -> LiveValues<Vec<bae_import::PendingFilterEntry>> {
    list.values_until_closed(list.subscribe_filter_entries())
}

/// What the rows the surface keeps selected in `list` hold and the actions
/// the person can run on them, whatever the list shows. The selection starts
/// empty, and the caller changes it on this same read.
pub fn import_list_selection(list: &LiveImportList) -> crate::live_reads::LiveImportSelection {
    crate::live_reads::LiveImportSelection::new(
        list.until_closed(list.subscribe_selection()),
        |_read, summary| Some(summary),
    )
}
