//! A clock on tokio's time, for the tests that pause it.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coven::{Clock, WallTimerError};

/// A clock whose `elapsed` is tokio's time since it was made and whose waits
/// are tokio's timers. On a paused runtime, tokio moves its time to the
/// earliest timer once every task waits, so waiters on this clock end in the
/// order of their ends, and a test measures them with tokio's `Instant`.
pub(crate) struct TokioClock {
    origin: tokio::time::Instant,
}

impl TokioClock {
    pub(crate) fn new() -> Self {
        Self { origin: tokio::time::Instant::now() }
    }
}

impl Clock for TokioClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + self.elapsed()
    }
    fn elapsed(&self) -> Duration {
        self.origin.elapsed()
    }
    /// Nothing these tests run waits for a wall-clock time.
    fn sleep_until(
        &self,
        _at: SystemTime,
    ) -> Pin<Box<dyn Future<Output = Result<(), WallTimerError>> + Send>> {
        Box::pin(std::future::ready(Err(WallTimerError::NoTimer)))
    }
}
