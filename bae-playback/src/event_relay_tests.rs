use super::*;
use bae_util::test_channels::HANG_GUARD;

/// A wake before the drain waits is kept: its next wait completes.
#[tokio::test]
async fn a_wake_before_the_wait_is_kept() {
    let (wake, relay) = event_relay(&Diagnostics::noop()).unwrap();

    wake.wake();

    tokio::time::timeout(HANG_GUARD, relay.ready()).await.expect("the wake was lost");
}

/// Dropping the drain waits for the relay thread to end: nothing of it is
/// left running, and the thread's hold on the state is gone.
#[test]
fn dropping_the_drain_joins_the_relay_thread() {
    let (_wake, relay) = event_relay(&Diagnostics::noop()).unwrap();
    let state = relay.state.clone();

    drop(relay);

    assert_eq!(Arc::strong_count(&state), 1, "the relay thread ended with the drop");
}
