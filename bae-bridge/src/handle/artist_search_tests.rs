//! The live artist search through the bridge object the platforms hold.

/// A search starts with no text, answers each `set_view` on the same
/// subscription with every window it named and the revision it answered, and
/// refuses a view once cancelled.
#[cfg(not(feature = "desktop"))]
#[test]
fn artist_search_answers_each_view_on_one_subscription() {
    let (handle, _root) = super::super::tests::fresh_bridge_handle("artist-search-views");
    let subscription = handle.subscribe_artist_search();

    let idle = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("idle search snapshot");
    assert_eq!(idle.request_revision, 0);
    assert_eq!(idle.total_count, 0);
    assert!(idle.windows.is_empty());

    let window = crate::types::BridgeLibraryPageWindow {
        offset: 0,
        limit: 50,
    };
    let revision = subscription
        .set_view("  Placeholder Query  ".to_string(), vec![window.clone()])
        .expect("set the search view");
    let searched = handle
        .runtime
        .block_on(subscription.clone().next())
        .expect("search snapshot");
    assert_eq!(searched.request_revision, revision);
    assert_eq!(searched.total_count, 0);
    assert_eq!(searched.windows.len(), 1);
    assert_eq!(searched.windows[0].window.limit, window.limit);

    handle
        .runtime
        .block_on(subscription.clone().cancel())
        .expect("cancel the search");
    assert!(matches!(
        subscription.set_view("Other Query".to_string(), vec![window]),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
