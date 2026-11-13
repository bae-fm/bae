//! The live library search through the bridge object the platforms hold.

/// A search starts with no query and answers each `set_query` on the same
/// subscription, carrying the query and revision it answered.
#[test]
fn library_search_answers_each_query_on_one_subscription() {
    let handle = super::super::tests::fresh_bridge_handle("library-search-queries");
    let subscription = handle.handle().subscribe_library_search();

    let idle = handle
        .handle()
        .runtime
        .block_on(subscription.clone().next())
        .expect("idle search snapshot");
    assert_eq!(idle.query, None);
    assert_eq!(idle.request_revision, 0);
    assert!(idle.results.albums.is_empty());

    let revision = subscription
        .set_query(Some("  Placeholder Query  ".to_string()))
        .expect("set the search query");
    let searched =
        handle.handle().runtime.block_on(subscription.clone().next()).expect("search snapshot");
    assert_eq!(searched.query.as_deref(), Some("Placeholder Query"));
    assert_eq!(searched.request_revision, revision);

    let cleared = subscription.set_query(None).expect("clear the search query");
    let cleared_snapshot =
        handle.handle().runtime.block_on(subscription.clone().next()).expect("cleared snapshot");
    assert_eq!(cleared_snapshot.query, None);
    assert_eq!(cleared_snapshot.request_revision, cleared);

    handle.handle().runtime.block_on(subscription.clone().cancel()).expect("cancel the search");
    assert!(matches!(
        subscription.set_query(Some("Other Query".to_string())),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
