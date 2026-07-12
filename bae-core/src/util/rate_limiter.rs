//! Minimum-interval rate limiter for provider API clients.

use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Enforces a minimum interval between calls. Each provider client holds
/// one `static` instance. The lock is held across the sleep, so concurrent
/// callers queue and each gets its own full-interval slot.
pub struct RateLimiter {
    interval: Duration,
    last_call: Mutex<Option<Instant>>,
}

impl RateLimiter {
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_call: Mutex::const_new(None),
        }
    }

    /// Sleep until `interval` has elapsed since the previous call, then
    /// stamp the current time. The first call returns immediately.
    pub async fn wait(&self) {
        let mut last_call = self.last_call.lock().await;
        if let Some(last) = *last_call {
            let elapsed = last.elapsed();
            if elapsed < self.interval {
                tokio::time::sleep(self.interval - elapsed).await;
            }
        }
        *last_call = Some(Instant::now());
    }

    /// Forget the previous call so the next `wait` returns immediately.
    /// Tests that share a static limiter reset it so one test's requests
    /// don't delay the next test's.
    #[cfg(test)]
    pub async fn reset(&self) {
        *self.last_call.lock().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn wait_spaces_calls_and_reset_clears_the_stamp() {
        let limiter = RateLimiter::new(Duration::from_secs(1));

        // First call returns immediately — no previous stamp.
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < Duration::from_millis(100));

        // Second call waits out the interval since the first.
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() >= Duration::from_millis(900));

        // After reset the next call is immediate again.
        limiter.reset().await;
        let start = Instant::now();
        limiter.wait().await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
