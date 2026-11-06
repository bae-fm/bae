//! The run loop's wait for a deadline the service's state carries: a running
//! side-pause countdown (`SidePauseDecision::resumes_at`), or the moment audio
//! still in an AirPlay receiver has been heard.
//!
//! The deadline itself lives in the state; this only holds the timer for it.
//! The loop hands it the state's deadline every turn, so the wait follows the
//! state: kept while the deadline is unchanged, re-armed for a new one, dropped
//! the moment the state no longer carries one.

use crate::{PlaybackClock, PlaybackSleep};
use chrono::{DateTime, Utc};

pub(super) struct DeadlineWait {
    armed: Option<(DateTime<Utc>, PlaybackSleep)>,
}

impl DeadlineWait {
    pub(super) fn new() -> Self {
        Self { armed: None }
    }

    /// Match the wait to the state's current deadline.
    pub(super) fn follow(&mut self, deadline: Option<DateTime<Utc>>, clock: &dyn PlaybackClock) {
        if self.armed.as_ref().map(|(armed, _)| *armed) != deadline {
            self.armed = deadline.map(|deadline| (deadline, clock.sleep_until(deadline)));
        }
    }

    pub(super) fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    /// Complete once the armed deadline passes, disarming and returning it.
    /// Never completes while nothing is armed. Dropping this future mid-wait
    /// keeps the wait armed for the next turn.
    pub(super) async fn elapsed(&mut self) -> DateTime<Utc> {
        let Some((deadline, sleep)) = self.armed.as_mut() else {
            return std::future::pending().await;
        };
        sleep.as_mut().await;
        let deadline = *deadline;
        self.armed = None;
        deadline
    }
}
