//! The playback service's time source: the wall-clock "now" a side-pause
//! countdown's deadline is set from, and the wait until that deadline.
//!
//! One trait for both halves because they must agree: a test that pins "now"
//! also has to decide when a wait is over, or a countdown would expire on real
//! time while its deadline was computed from fake time. Production reads the
//! library's injected wall clock and waits on the service runtime's timer; tests
//! move a [`ManualPlaybackClock`] forward by hand.

use chrono::{DateTime, Utc};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A wait that completes once its deadline has passed.
pub type PlaybackSleep = Pin<Box<dyn Future<Output = ()> + Send>>;

pub trait PlaybackClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    /// Complete once `now()` has reached `deadline` — at once if it already has.
    fn sleep_until(&self, deadline: DateTime<Utc>) -> PlaybackSleep;
}

pub type PlaybackClockRef = Arc<dyn PlaybackClock>;

/// Production: the library's wall clock for "now", the service runtime's timer
/// for the wait. The wait is measured once, when it starts, as the time left
/// until the deadline, so a wall-clock correction during a countdown doesn't
/// stretch or cut it.
pub(crate) struct WallPlaybackClock {
    wall: coven::ClockRef,
}

impl WallPlaybackClock {
    pub(crate) fn new(wall: coven::ClockRef) -> Self {
        Self { wall }
    }
}

impl PlaybackClock for WallPlaybackClock {
    fn now(&self) -> DateTime<Utc> {
        self.wall.now()
    }

    fn sleep_until(&self, deadline: DateTime<Utc>) -> PlaybackSleep {
        let remaining = (deadline - self.wall.now())
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        Box::pin(tokio::time::sleep(remaining))
    }
}

/// A clock that moves only when a test calls [`Self::advance`]. Every pending
/// [`PlaybackClock::sleep_until`] whose deadline the move reaches completes.
#[cfg(any(test, feature = "test-utils"))]
pub struct ManualPlaybackClock {
    now: tokio::sync::watch::Sender<DateTime<Utc>>,
    /// One entry per wait handed out; it upgrades while the wait is pending.
    waits: std::sync::Mutex<Vec<std::sync::Weak<DateTime<Utc>>>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl ManualPlaybackClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        let (now, _) = tokio::sync::watch::channel(start);
        Self {
            now,
            waits: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The deadlines of the waits handed out that have neither completed nor
    /// been dropped, in the order they were handed out. A holder that gives up
    /// a wait drops it, so this is what the clock's users are still waiting on.
    pub fn pending_waits(&self) -> Vec<DateTime<Utc>> {
        let mut waits = self.waits.lock().unwrap();
        waits.retain(|wait| wait.strong_count() > 0);
        waits
            .iter()
            .filter_map(|wait| wait.upgrade().map(|deadline| *deadline))
            .collect()
    }

    pub fn advance(&self, by: std::time::Duration) {
        let by = chrono::Duration::from_std(by).expect("test advance fits a chrono duration");
        self.now.send_modify(|now| *now += by);
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl PlaybackClock for ManualPlaybackClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.borrow()
    }

    fn sleep_until(&self, deadline: DateTime<Utc>) -> PlaybackSleep {
        let mut now = self.now.subscribe();
        let wait = Arc::new(deadline);
        self.waits.lock().unwrap().push(Arc::downgrade(&wait));
        Box::pin(async move {
            // The sender lives as long as the clock; a clock dropped mid-wait
            // never reaches the deadline, so the wait stays pending.
            if now.wait_for(|now| *now >= deadline).await.is_err() {
                std::future::pending::<()>().await;
            }
            drop(wait);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        tokio::time::timeout(Duration::from_secs(1), sleep)
            .await
            .expect("the wait completes at the deadline");
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
        assert_eq!(
            clock.pending_waits(),
            vec![start() + chrono::Duration::seconds(30), soon, later]
        );

        drop(dropped);
        clock.advance(Duration::from_secs(5));
        tokio::time::timeout(Duration::from_secs(1), &mut completes)
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
        tokio::time::timeout(Duration::from_secs(1), clock.sleep_until(start()))
            .await
            .expect("a deadline already reached needs no wait");
    }
}
