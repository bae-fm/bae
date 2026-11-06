use super::*;
use bae_util::test_channels::HANG_GUARD;

/// A wake before the drain waits is kept: its next wait completes.
#[tokio::test]
async fn a_wake_before_the_wait_is_kept() {
    let (wake, relay) = event_relay().unwrap();

    wake.wake();

    tokio::time::timeout(HANG_GUARD, relay.ready()).await.expect("the wake was lost");
}
