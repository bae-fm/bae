//! The artist picker's search through the bridge objects the platforms hold:
//! its text is part of the list, so a new text is a new subscription.

use crate::types::{BridgeList, BridgeListRows, BridgeListWindow};
use std::sync::Arc;

fn searching(text: &str) -> BridgeList {
    BridgeList::ArtistSearch { text: text.to_string() }
}

fn top(limit: u64) -> BridgeListWindow {
    BridgeListWindow::Forward { from: None, until: None, limit }
}

/// A search answers its window and its count, a blank text finds nothing, and
/// a cancelled window refuses a move.
#[test]
fn artist_search_answers_its_window_and_count_and_blank_finds_nothing() {
    let handle = super::super::tests::fresh_bridge_handle("artist-search-lists");
    let runtime = &handle.handle().runtime;

    let window = handle.handle().subscribe_list_window(searching("  Placeholder Query  "), top(50));
    let value = runtime.block_on(window.clone().next()).expect("search window value");
    assert_eq!(value.window, top(50));
    assert!(matches!(value.rows, BridgeListRows::ArtistSearch { ref rows } if rows.is_empty()));
    let count = handle.handle().subscribe_list_count(searching("Placeholder Query"));
    assert_eq!(runtime.block_on(count.clone().next()).expect("search count").rows, 0);

    let blank = handle.handle().subscribe_list_window(searching("   "), top(50));
    let nothing = runtime.block_on(blank.clone().next()).expect("blank search window value");
    assert!(matches!(nothing.rows, BridgeListRows::ArtistSearch { ref rows } if rows.is_empty()));
    assert!(runtime
        .block_on(Arc::clone(handle.handle()).list_key_at(searching("   "), 0))
        .expect("blank search key")
        .is_none());

    runtime.block_on(window.clone().cancel()).expect("cancel the search window");
    assert!(matches!(window.set_window(top(10)), Err(crate::types::BridgeError::Cancelled)));
}
