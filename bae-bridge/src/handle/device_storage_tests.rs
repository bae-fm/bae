//! What this device keeps, as the app pulls it.

use super::super::tests::pull_until;
use crate::types::{BridgeCacheLimitChoice, BridgeStorageKind};

/// A new library holds only its own files, offers every choice for how much
/// audio to keep and starts at 20 GB; Keep up to follows its setter, and
/// Clear Cache and deleting what a reset kept are calls that succeed with
/// nothing to remove.
#[test]
fn the_storage_follows_keep_up_to_and_its_commands_answer() {
    let handle = super::super::tests::fresh_bridge_handle("device-storage-pulls");
    let values = handle.handle().subscribe_device_storage();

    let first = pull_until(handle.handle(), || values.clone().next(), |_| true);
    assert_eq!(
        first.parts.iter().map(|part| part.kind).collect::<Vec<_>>(),
        [BridgeStorageKind::Library],
        "{first:?}"
    );
    assert_eq!(first.total, first.parts[0].bytes);
    assert!(!first.can_clear_cache);
    assert_eq!(first.before_reset, None);
    assert_eq!(first.cache_limit.choice, BridgeCacheLimitChoice::Gb20);
    assert_eq!(first.cache_limit.bytes, 20_000_000_000);
    assert_eq!(
        first.cache_limits.iter().map(|limit| limit.bytes).collect::<Vec<_>>(),
        [1_000_000_000, 5_000_000_000, 10_000_000_000, 20_000_000_000, 50_000_000_000]
    );

    let runtime = &handle.handle().runtime;
    runtime.block_on(handle.handle().clone().set_cache_limit(BridgeCacheLimitChoice::Gb5)).unwrap();
    let changed = pull_until(
        handle.handle(),
        || values.clone().next(),
        |value| value.cache_limit.choice == BridgeCacheLimitChoice::Gb5,
    );
    assert_eq!(changed.cache_limit.bytes, 5_000_000_000);

    runtime.block_on(handle.handle().clone().clear_cache()).unwrap();
    runtime.block_on(handle.handle().clone().delete_data_from_before_reset()).unwrap();

    runtime.block_on(values.clone().cancel()).unwrap();
    assert!(matches!(
        runtime.block_on(values.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
