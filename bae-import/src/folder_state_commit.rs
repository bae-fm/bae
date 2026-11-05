//! The lock every write to a candidate's folder state is taken under.
//!
//! A scan storing what it read, a person's pane control, an identification
//! storing its verdict and an import claiming its candidate all take it, so
//! how long any one of them holds it is how long every other one waits. Each
//! holder names what it is doing; releasing the lock logs how long that
//! operation waited for it and held it, and warns when a hold was long enough
//! for a click queued behind it to be felt.

use std::sync::Arc;
use std::time::Duration;
// Tokio's clock, so a test can hold the lock for a given time without waiting it.
use tokio::time::Instant;

/// A hold at least this long is a click queued behind it that the person
/// notices, and is logged as a warning naming the operation that held it.
const NOTICEABLE_HOLD: Duration = Duration::from_millis(250);

/// The folder-state commit lock. Clones share the one lock.
#[derive(Clone, Default)]
pub(crate) struct FolderStateCommit {
    lock: Arc<tokio::sync::Mutex<()>>,
    #[cfg(test)]
    waiting: Arc<Waiting>,
}

/// How many callers are waiting for the lock, for a test to wait on.
#[cfg(test)]
#[derive(Default)]
struct Waiting {
    count: std::sync::atomic::AtomicUsize,
    changed: tokio::sync::Notify,
}

impl FolderStateCommit {
    /// Wait for the lock and hold it for `operation`, which names the holder
    /// in the log lines its release writes.
    pub(crate) async fn lock(&self, operation: &'static str) -> FolderStateCommitGuard {
        let asked = Instant::now();
        #[cfg(test)]
        self.waiting.arrive();
        let guard = self.lock.clone().lock_owned().await;
        #[cfg(test)]
        self.waiting.leave();
        let acquired = Instant::now();
        FolderStateCommitGuard { _guard: guard, operation, waited: acquired - asked, acquired }
    }
}

#[cfg(test)]
impl Waiting {
    fn arrive(&self) {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    fn leave(&self) {
        self.count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        self.changed.notify_waiters();
    }
}

#[cfg(test)]
impl FolderStateCommit {
    /// Wait until `count` callers are waiting for the lock.
    pub(crate) async fn waiting_reaches(&self, count: usize) {
        loop {
            let changed = self.waiting.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.waiting.count.load(std::sync::atomic::Ordering::SeqCst) >= count {
                return;
            }
            changed.await;
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
#[path = "folder_state_commit_tests.rs"]
mod tests;
