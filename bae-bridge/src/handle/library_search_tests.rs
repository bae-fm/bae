//! The live library search through the bridge object the platforms hold.

/// A search starts with no query and answers each `set_query` on the same
/// subscription, carrying the query and revision it answered.
#[cfg(not(feature = "desktop"))]
#[test]
fn library_search_answers_each_query_on_one_subscription() {
    let (handle, _root) = super::super::tests::fresh_bridge_handle("library-search-queries");
    let subscription = handle.subscribe_library_search();

    let idle = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("idle search snapshot");
    assert_eq!(idle.query, "");
    assert_eq!(idle.request_revision, 0);
    assert!(idle.results.albums.is_empty());

    let revision = subscription
        .set_query("  Placeholder Query  ".to_string())
        .expect("set the search query");
    let searched = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("search snapshot");
    assert_eq!(searched.query, "Placeholder Query");
    assert_eq!(searched.request_revision, revision);

    handle
        .runtime
        .block_on(subscription.clone().cancel())
        .expect("cancel the search");
    assert!(matches!(
        subscription.set_query("Other Query".to_string()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
