//! These tests check the artist picker's search through the bridge objects
//! the apps hold. The text is part of the list, so each new text opens a new
//! subscription.

use crate::types::{BridgeList, BridgeListRows};

fn searching(text: &str) -> BridgeList {
    BridgeList::ArtistSearch { text: text.to_string() }
}

/// A search answers its view and its count, a blank text finds nothing, and
/// a cancelled view answers that it was cancelled.
#[test]
fn artist_search_answers_its_view_and_count_and_blank_finds_nothing() {
    let handle = super::super::tests::fresh_bridge_handle("artist-search-lists");
    let runtime = &handle.handle().runtime;

    let window = handle.handle().subscribe_list_view(searching("  Placeholder Query  "), 0, 9);
    let value = runtime.block_on(window.clone().next()).expect("search view value");
    assert_eq!(value.start, 0);
    assert!(matches!(value.rows, BridgeListRows::ArtistSearch { ref rows } if rows.is_empty()));
    let count = handle.handle().subscribe_list_count(searching("Placeholder Query"));
    assert_eq!(runtime.block_on(count.clone().next()).expect("search count").rows, 0);

    let blank = handle.handle().subscribe_list_view(searching("   "), 0, 9);
    let nothing = runtime.block_on(blank.clone().next()).expect("blank search view value");
    assert!(matches!(nothing.rows, BridgeListRows::ArtistSearch { ref rows } if rows.is_empty()));

    runtime.block_on(window.clone().cancel()).expect("cancel the search view");
    assert!(matches!(
        runtime.block_on(window.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
