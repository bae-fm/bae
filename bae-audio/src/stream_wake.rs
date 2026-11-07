//! The wake a thread that drains a player's track streams waits on.
//!
//! A drain that runs on its own thread (Android's blocking writer, the AirPlay
//! sender, a test capture) has nothing to do while the player is paused or
//! the ring it reads is empty. It reads [`StreamWake::changes`] before it
//! looks, and when it finds nothing it waits with [`StreamWake::wait_past`]
//! until something that could give it work happens after that read: a
//! decoder pushed samples or finished, a track was put in its source, the
//! player's state changed, or its owner stopped it. A change that lands
//! between the read and the wait ends the wait at once, so none is missed.
//!
//! The device's real-time callback never waits on it and never notifies it.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

/// Counts the changes that could give a player's drain work. Clones share one
/// count.
#[derive(Clone)]
pub struct StreamWake {
    shared: Arc<WakeState>,
}

struct WakeState {
    counts: Mutex<WakeCounts>,
    changed: Condvar,
}

#[derive(Default)]
struct WakeCounts {
    changes: u64,
    /// Threads in [`StreamWake::wait_past`] now.
    waiting: usize,
}

impl StreamWake {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(WakeState {
                counts: Mutex::new(WakeCounts::default()),
                changed: Condvar::new(),
            }),
        }
    }

    fn counts(&self) -> MutexGuard<'_, WakeCounts> {
        self.shared.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count one change and wake every waiter.
    pub fn notify(&self) {
        self.counts().changes += 1;
        self.shared.changed.notify_all();
    }

    /// The changes counted so far. Read before looking for work, and hand to
    /// [`Self::wait_past`] when there was none.
    pub fn changes(&self) -> u64 {
        self.counts().changes
    }

    /// Wait until a change past `seen` has been counted.
    pub fn wait_past(&self, seen: u64) {
        let mut counts = self.counts();
        if counts.changes != seen {
            return;
        }
        counts.waiting += 1;
        self.shared.changed.notify_all();
        let mut counts = self
            .shared
            .changed
            .wait_while(counts, |counts| counts.changes == seen)
            .unwrap_or_else(PoisonError::into_inner);
        counts.waiting -= 1;
    }

    /// Test-only: wait until a thread is waiting for a change. Panics if the
    /// hang guard passes first.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn wait_until_waiting(&self) {
        let (_counts, result) = self
            .shared
            .changed
            .wait_timeout_while(self.counts(), bae_util::test_channels::HANG_GUARD, |counts| {
                counts.waiting == 0
            })
            .unwrap_or_else(PoisonError::into_inner);
        assert!(!result.timed_out(), "no thread ever waited for a change");
    }
}

#[cfg(test)]
#[path = "stream_wake_tests.rs"]
mod tests;
