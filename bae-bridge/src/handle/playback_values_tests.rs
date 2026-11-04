//! What playback is doing, as the app pulls it.

use crate::types::{BridgePlaybackValueState, BridgePlaybackValues};
use bae_util::test_channels::HANG_GUARD;

/// The first value `values` answers that `wanted` takes.
fn pull_until(
    handle: &super::AppHandle,
    values: &std::sync::Arc<crate::PlaybackValuesSubscription>,
    wanted: impl Fn(&BridgePlaybackValues) -> bool,
) -> BridgePlaybackValues {
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

/// The values start stopped at full volume with no failure and follow each
/// change playback makes; cancelling ends the pulls.
#[test]
fn playback_values_follow_playback_until_cancelled() {
    let (handle, _root) = super::super::tests::fresh_bridge_handle("playback-values");
    let values = handle.subscribe_playback_values();

    let first = pull_until(&handle, &values, |_| true);
    assert!(matches!(first.state, BridgePlaybackValueState::Stopped));
    assert_eq!((first.volume, first.is_muted), (1.0, false));
    assert!(first.failure.is_none());

    handle.set_volume(0.4);
    pull_until(&handle, &values, |value| value.volume == 0.4);
    handle.set_muted(true);
    pull_until(&handle, &values, |value| value.is_muted);

    handle.runtime.block_on(values.clone().cancel()).unwrap();
    assert!(matches!(
        handle.runtime.block_on(values.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
