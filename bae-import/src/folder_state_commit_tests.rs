#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

/// Everything logged at `level` and above by one hold of the lock that
/// lasts `held_for`.
fn logs_of_a_hold(level: tracing::Level, held_for: Duration) -> String {
    bae_util::test_logs::capture_logs_at(level, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(async {
                let commit = FolderStateCommit::new();
                let guard = commit.lock("store scan items").await;
                tokio::time::advance(held_for).await;
                drop(guard);
            });
    })
}

#[test]
fn a_long_hold_warns_naming_the_operation_that_held_it() {
    let logs = logs_of_a_hold(tracing::Level::WARN, NOTICEABLE_HOLD + Duration::from_millis(20));
    assert!(logs.contains("WARN"), "{logs}");
    assert!(logs.contains("operation=\"store scan items\""), "{logs}");
    assert!(logs.contains("held_ms="), "{logs}");
    assert!(logs.contains("waited_ms="), "{logs}");
}

#[test]
fn a_short_hold_is_logged_at_debug_only() {
    assert_eq!(logs_of_a_hold(tracing::Level::WARN, Duration::ZERO), "");
    let logs = logs_of_a_hold(tracing::Level::DEBUG, Duration::ZERO);
    assert!(logs.contains("DEBUG"), "{logs}");
    assert!(logs.contains("operation=\"store scan items\""), "{logs}");
}

#[tokio::test(start_paused = true)]
async fn the_wait_is_how_long_the_lock_was_held_by_another() {
    let commit = FolderStateCommit::new();
    let first = commit.lock("hold for a test").await;
    let waiter = tokio::spawn({
        let commit = commit.clone();
        async move { commit.lock("wait for a test").await.waited }
    });
    // On this one-thread runtime the waiter runs when the test yields, and
    // parks on the lock after taking the time it asked.
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(30)).await;
    drop(first);
    assert_eq!(waiter.await.unwrap(), Duration::from_millis(30));
}
