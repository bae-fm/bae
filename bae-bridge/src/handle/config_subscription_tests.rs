//! This device's preferences, as the settings screens pull them.

use crate::types::BridgeConfig;
use bae_util::test_channels::HANG_GUARD;

/// The first value `values` answers that `wanted` takes.
fn pull_until(
    handle: &super::AppHandle,
    values: &std::sync::Arc<crate::ConfigSubscription>,
    wanted: impl Fn(&BridgeConfig) -> bool,
) -> BridgeConfig {
    handle.runtime.block_on(async {
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                let value = values.clone().next().await.expect("the pull reads");
                if wanted(&value) {
                    return value;
                }
            }
        })
        .await
        .expect("the values get there")
    })
}

/// The preferences start at a new library's, with core's transfer limit, and
/// follow each setter's write; cancelling ends the pulls.
#[test]
fn the_preferences_follow_each_write_until_cancelled() {
    let (handle, _root) = super::super::tests::fresh_bridge_handle("config-pulls");
    let values = handle.subscribe_config();

    let first = pull_until(&handle, &values, |_| true);
    assert!(!first.show_remaining_time);
    assert_eq!(first.max_concurrent_uploads, 3);
    assert_eq!(
        first.max_concurrent_transfers,
        bae_preferences::MAX_CONCURRENT_TRANSFERS,
        "the pickers offer up to core's limit"
    );
    assert_eq!(first.library_id.len(), 36, "{}", first.library_id);

    handle.runtime.block_on(handle.clone().set_show_remaining_time(true)).unwrap();
    pull_until(&handle, &values, |value| value.show_remaining_time);
    handle.runtime.block_on(handle.clone().set_max_concurrent_uploads(5)).unwrap();
    let changed = pull_until(&handle, &values, |value| value.max_concurrent_uploads == 5);
    assert!(changed.show_remaining_time, "the earlier write stands");

    handle.runtime.block_on(values.clone().cancel()).unwrap();
    assert!(matches!(
        handle.runtime.block_on(values.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
