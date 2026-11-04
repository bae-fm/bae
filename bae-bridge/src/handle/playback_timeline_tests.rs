//! Where playback is, as the surfaces that move the position on themselves
//! pull it.

use bae_util::test_channels::HANG_GUARD;

/// A library that has played nothing reads no line and no seek; cancelling
/// ends the pulls.
#[test]
fn the_timeline_starts_with_no_line_until_cancelled() {
    let (handle, _root) = super::super::tests::fresh_bridge_handle("playback-timeline");
    let timeline = handle.subscribe_playback_timeline();

    let first = handle
        .runtime
        .block_on(async { tokio::time::timeout(HANG_GUARD, timeline.clone().next()).await })
        .expect("the first value in time")
        .expect("the pull reads");
    assert_eq!(
        first,
        crate::types::BridgePlaybackTimeline { library: None, seek_revision: 0, preview: None }
    );

    handle.runtime.block_on(timeline.clone().cancel()).unwrap();
    assert!(matches!(
        handle.runtime.block_on(timeline.clone().next()),
        Err(crate::types::BridgeError::Cancelled)
    ));
}
