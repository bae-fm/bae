//! The lock every write to a candidate's folder state is taken under.
//!
//! A scan storing what it read, a person's pane control, an identification
//! storing its verdict and an import claiming its candidate all take it, so
//! how long any one of them holds it is how long every other one waits. Each
//! holder names what it is doing; releasing the lock logs how long that
//! operation waited for it and held it, and warns when a hold was long enough
//! for a click queued behind it to be felt.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// A hold at least this long is a click queued behind it that the person
/// notices, and is logged as a warning naming the operation that held it.
const NOTICEABLE_HOLD: Duration = Duration::from_millis(250);

/// The folder-state commit lock. Clones share the one lock.
#[derive(Clone, Default)]
pub(crate) struct FolderStateCommit(Arc<tokio::sync::Mutex<()>>);

impl FolderStateCommit {
    /// Wait for the lock and hold it for `operation`, which names the holder
    /// in the log lines its release writes.
    pub(crate) async fn lock(&self, operation: &'static str) -> FolderStateCommitGuard {
        let asked = Instant::now();
        let guard = self.0.clone().lock_owned().await;
        let acquired = Instant::now();
        FolderStateCommitGuard {
            _guard: guard,
            operation,
            waited: acquired - asked,
            acquired,
        }
    }
}

/// The lock, held for one operation until this is dropped.
pub(crate) struct FolderStateCommitGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    operation: &'static str,
    waited: Duration,
    acquired: Instant,
}

impl Drop for FolderStateCommitGuard {
    fn drop(&mut self) {
        let held = self.acquired.elapsed();
        let waited_ms = self.waited.as_millis() as u64;
        let held_ms = held.as_millis() as u64;
        if held >= NOTICEABLE_HOLD {
            tracing::warn!(
                operation = self.operation,
                waited_ms,
                held_ms,
                "folder-state commit lock held long enough to delay what waits on it"
            );
        } else {
            tracing::debug!(
                operation = self.operation,
                waited_ms,
                held_ms,
                "folder-state commit lock released"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything logged at `level` and above by one hold of the lock that
    /// lasts `held_for`.
    fn logs_of_a_hold(level: tracing::Level, held_for: Duration) -> String {
        crate::test_logs::capture_logs_at(level, || {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    let commit = FolderStateCommit::default();
                    let guard = commit.lock("store a scan item").await;
                    std::thread::sleep(held_for);
                    drop(guard);
                });
        })
    }

    #[test]
    fn a_long_hold_warns_naming_the_operation_that_held_it() {
        let logs = logs_of_a_hold(tracing::Level::WARN, NOTICEABLE_HOLD + Duration::from_millis(20));
        assert!(logs.contains("WARN"), "{logs}");
        assert!(logs.contains("operation=\"store a scan item\""), "{logs}");
        assert!(logs.contains("held_ms="), "{logs}");
        assert!(logs.contains("waited_ms="), "{logs}");
    }

    #[test]
    fn a_short_hold_is_logged_at_debug_only() {
        assert_eq!(logs_of_a_hold(tracing::Level::WARN, Duration::ZERO), "");
        let logs = logs_of_a_hold(tracing::Level::DEBUG, Duration::ZERO);
        assert!(logs.contains("DEBUG"), "{logs}");
        assert!(logs.contains("operation=\"store a scan item\""), "{logs}");
    }

    #[tokio::test]
    async fn the_wait_is_how_long_the_lock_was_held_by_another() {
        let commit = FolderStateCommit::default();
        let first = commit.lock("hold for a test").await;
        let waiter = tokio::spawn({
            let commit = commit.clone();
            async move { commit.lock("wait for a test").await.waited }
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        drop(first);
        assert!(waiter.await.unwrap() >= Duration::from_millis(30));
    }
}
