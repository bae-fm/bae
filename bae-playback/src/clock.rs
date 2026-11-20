//! The playback service's time source: the wall-clock "now" a side-pause
//! countdown's deadline is set from, and the wait until that deadline.
//!
//! One trait for both halves because they must agree: a test that pins "now"
//! also has to decide when a wait is over, or a countdown would expire on real
//! time while its deadline was computed from fake time. Production reads the
//! library's injected wall clock and waits on the service runtime's timer; tests
//! move a [`ManualPlaybackClock`] forward by hand.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use time::UtcDateTime;

/// A wait that completes once its deadline has passed.
pub type PlaybackSleep = Pin<Box<dyn Future<Output = ()> + Send>>;

pub trait PlaybackClock: Send + Sync {
    fn now(&self) -> UtcDateTime;
    /// Complete once `now()` has reached `deadline` — at once if it already has.
    fn sleep_until(&self, deadline: UtcDateTime) -> PlaybackSleep;
}

pub type PlaybackClockRef = Arc<dyn PlaybackClock>;

/// The time zone the device is in, which the app tells playback: the offset
/// from UTC its clock shows at a moment, daylight saving time included.
pub trait LocalZone: Send + Sync + std::fmt::Debug {
    fn offset_at(&self, at: UtcDateTime) -> Result<time::UtcOffset, LocalZoneError>;
}

/// The device's time zone gave no usable offset: the app told none yet, or the
/// app's answer was not an offset.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct LocalZoneError(pub String);

pub type LocalZoneRef = Arc<dyn LocalZone>;

/// UTC as a device's zone, for tests that have no app to tell one.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct UtcZone;

#[cfg(any(test, feature = "test-utils"))]
impl LocalZone for UtcZone {
    fn offset_at(&self, _at: UtcDateTime) -> Result<time::UtcOffset, LocalZoneError> {
        Ok(time::UtcOffset::UTC)
    }
}

/// Production: the library's wall clock for "now", the service runtime's timer
/// for the wait. The wait is measured once, when it starts, as the time left
/// until the deadline, so a wall-clock correction during a countdown doesn't
/// stretch or cut it.
pub struct WallPlaybackClock {
    wall: coven::ClockRef,
}

impl WallPlaybackClock {
    pub fn new(wall: coven::ClockRef) -> Self {
        Self { wall }
    }
}

impl PlaybackClock for WallPlaybackClock {
    fn now(&self) -> UtcDateTime {
        self.wall.now().into()
    }

    fn sleep_until(&self, deadline: UtcDateTime) -> PlaybackSleep {
        let remaining = std::time::Duration::try_from(deadline - self.now())
            .unwrap_or(std::time::Duration::ZERO);
        self.wall.deadline(remaining)
    }
}

/// A clock that moves only when a test calls [`Self::advance`]. Every pending
/// [`PlaybackClock::sleep_until`] whose deadline the move reaches completes.
#[cfg(any(test, feature = "test-utils"))]
pub struct ManualPlaybackClock {
    now: tokio::sync::watch::Sender<UtcDateTime>,
    /// One entry per wait handed out; it upgrades while the wait is pending.
    waits: std::sync::Mutex<Vec<std::sync::Weak<UtcDateTime>>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl ManualPlaybackClock {
    pub fn new(start: UtcDateTime) -> Self {
        let (now, _) = tokio::sync::watch::channel(start);
        Self { now, waits: std::sync::Mutex::new(Vec::new()) }
    }

    /// The deadlines of the waits handed out that have neither completed nor
    /// been dropped, in the order they were handed out. A holder that gives up
    /// a wait drops it, so this is what the clock's users are still waiting on.
    pub fn pending_waits(&self) -> Vec<UtcDateTime> {
        let mut waits = self.waits.lock().unwrap();
        waits.retain(|wait| wait.strong_count() > 0);
        waits.iter().filter_map(|wait| wait.upgrade().map(|deadline| *deadline)).collect()
    }

    pub fn advance(&self, by: std::time::Duration) {
        let by = time::Duration::try_from(by).expect("a test advance fits a time::Duration");
        self.now.send_modify(|now| *now += by);
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl PlaybackClock for ManualPlaybackClock {
    fn now(&self) -> UtcDateTime {
        *self.now.borrow()
    }

    fn sleep_until(&self, deadline: UtcDateTime) -> PlaybackSleep {
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
#[path = "clock_tests.rs"]
mod tests;
