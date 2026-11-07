//! What playback is doing, as the app pulls it.

use crate::types::BridgePlaybackValueState;

use super::super::tests::pull_until;

/// The values start stopped at full volume with no failure and follow each
/// change playback makes; cancelling ends the pulls.
#[test]
fn playback_values_follow_playback_until_cancelled() {
    let handle = super::super::tests::fresh_bridge_handle("playback-values");
    let values = handle.handle().subscribe_playback_values();

    let first = pull_until(handle.handle(), || values.clone().next(), |_| true);
    assert!(matches!(first.state, BridgePlaybackValueState::Stopped));
    assert_eq!((first.volume, first.is_muted), (1.0, false));
    assert!(first.failure.is_none());

    handle.handle().set_volume(0.4);
    pull_until(handle.handle(), || values.clone().next(), |value| value.volume == 0.4);
    handle.handle().set_muted(true);
    pull_until(handle.handle(), || values.clone().next(), |value| value.is_muted);

    handle.handle().runtime.block_on(values.clone().cancel()).unwrap();
    assert!(matches!(
        handle.handle().runtime.block_on(values.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
