//! A live import list read from its last read's rows only while none of them
//! has moved.

use super::super::*;
use bae_db::{LibraryPageWindow, LibraryPageWindows};

/// The import list's items in the windows `request` names, as one read with
/// no earlier read's rows to start from finds them.
async fn import_items(
    db: &Database,
    request: crate::ImportListRequest,
) -> Vec<Vec<crate::list::ImportListItem>> {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .load_import_list(request)
        .await
        .unwrap()
        .windows
        .into_iter()
        .map(|window| window.items)
        .collect()
}

/// A live import list of grouped folders, scrolled a window at a time down
/// and back up, holds at each window what a read with nothing to start from
/// holds, its members' headers included; and so it does after a scan adds
/// folders above the window it scrolls from.
#[tokio::test(flavor = "multi_thread")]
async fn a_scrolled_import_list_holds_its_offsets_items_across_a_scan() {
    use crate::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    let (db, tmp) = super::empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    super::import_list_window_cost::grow_import_list(&db, tmp.path(), 0, 300).await;
    let list = tables
        .open_import_list(ImportListView {
            tab: crate::TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
            // The folders a later scan adds sort first.
            order: ImportListOrder::PathDescending,
        })
        .await
        .unwrap();
    let request = |offset| ImportListRequest {
        list,
        windows: LibraryPageWindows::from([LibraryPageWindow { offset, limit: 10 }]),
    };
    let mut query = tables.subscribe_import_list(request(0));
    query.next().await.unwrap().result.unwrap();
    let requests = query.requests();
    let mut scroll = async |offset: u64| {
        requests.set(request(offset)).unwrap();
        loop {
            let event = query.next().await.unwrap();
            if event.request == request(offset) {
                return event
                    .result
                    .unwrap()
                    .windows
                    .into_iter()
                    .map(|window| window.items)
                    .collect::<Vec<_>>();
            }
        }
    };
    for offset in [10, 20, 30, 40, 30, 20, 10] {
        assert_eq!(scroll(offset).await, import_items(&db, request(offset)).await, "at {offset}");
    }
    super::import_list_window_cost::grow_import_list(&db, tmp.path(), 300, 305).await;
    for offset in [30, 20] {
        assert_eq!(
            scroll(offset).await,
            import_items(&db, request(offset)).await,
            "at {offset} after the scan"
        );
    }
}
