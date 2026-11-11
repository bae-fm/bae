//! This device's preferences, as the settings screens pull them.

use super::super::tests::pull_until;

/// The preferences start at a new library's, with core's transfer limit, and
/// follow each setter's write; cancelling ends the pulls.
#[test]
fn the_preferences_follow_each_write_until_cancelled() {
    let handle = super::super::tests::fresh_bridge_handle("config-pulls");
    let values = handle.handle().subscribe_config();

    let first = pull_until(handle.handle(), || values.clone().next(), |_| true);
    assert!(!first.show_remaining_time);
    assert!(first.ask_before_unpinning, "Unpin asks first until the person says not to");
    assert_eq!(first.max_concurrent_uploads, 3);
    assert_eq!(
        first.max_concurrent_transfers,
        bae_preferences::MAX_CONCURRENT_TRANSFERS,
        "the pickers offer up to core's limit"
    );
    assert_eq!(first.library_id.len(), 36, "{}", first.library_id);

    handle
        .handle()
        .runtime
        .block_on(handle.handle().clone().set_show_remaining_time(true))
        .unwrap();
    pull_until(handle.handle(), || values.clone().next(), |value| value.show_remaining_time);
    handle
        .handle()
        .runtime
        .block_on(handle.handle().clone().set_max_concurrent_uploads(5))
        .unwrap();
    let changed = pull_until(
        handle.handle(),
        || values.clone().next(),
        |value| value.max_concurrent_uploads == 5,
    );
    assert!(changed.show_remaining_time, "the earlier write stands");
    handle
        .handle()
        .runtime
        .block_on(handle.handle().clone().set_ask_before_unpinning(false))
        .unwrap();
    let unasked =
        pull_until(handle.handle(), || values.clone().next(), |value| !value.ask_before_unpinning);
    assert!(unasked.show_remaining_time, "the earlier writes stand");

    handle.handle().runtime.block_on(values.clone().cancel()).unwrap();
    assert!(matches!(
        handle.handle().runtime.block_on(values.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
