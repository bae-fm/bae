use super::*;
use bae_util::test_channels::HANG_GUARD;
use std::time::Duration;

fn start() -> DateTime<Utc> {
    "2026-01-01T00:00:00Z".parse().unwrap()
}

#[tokio::test]
async fn manual_clock_wait_completes_only_once_the_deadline_is_reached() {
    let clock = ManualPlaybackClock::new(start());
    let deadline = start() + chrono::Duration::seconds(5);
    let mut sleep = clock.sleep_until(deadline);

    clock.advance(Duration::from_secs(4));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        sleep.as_mut().poll(&mut cx).is_pending(),
        "a second before the deadline the wait is still pending"
    );

    clock.advance(Duration::from_secs(1));
    tokio::time::timeout(HANG_GUARD, sleep).await.expect("the wait completes at the deadline");
    assert_eq!(clock.now(), deadline);
}

#[tokio::test]
async fn manual_clock_lists_only_the_waits_still_pending() {
    let clock = ManualPlaybackClock::new(start());
    let soon = start() + chrono::Duration::seconds(5);
    let later = start() + chrono::Duration::seconds(60);
    let dropped = clock.sleep_until(start() + chrono::Duration::seconds(30));
    let mut completes = clock.sleep_until(soon);
    let _held = clock.sleep_until(later);
    assert_eq!(clock.pending_waits(), vec![start() + chrono::Duration::seconds(30), soon, later]);

    drop(dropped);
    clock.advance(Duration::from_secs(5));
    tokio::time::timeout(HANG_GUARD, &mut completes)
        .await
        .expect("the wait completes at its deadline");
    assert_eq!(
        clock.pending_waits(),
        vec![later],
        "a dropped wait and a completed one (still held) are no longer pending"
    );
}

#[tokio::test]
async fn manual_clock_wait_for_a_passed_deadline_completes_at_once() {
    let clock = ManualPlaybackClock::new(start());
    tokio::time::timeout(HANG_GUARD, clock.sleep_until(start()))
        .await
        .expect("a deadline already reached needs no wait");
}
