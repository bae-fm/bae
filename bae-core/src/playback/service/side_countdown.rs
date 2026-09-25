//! The run loop's wait for a running side-pause countdown.
//!
//! The countdown itself lives in the side-pause phase (`SidePauseDecision::
//! resumes_at`); this only holds the timer for it. The loop hands it the
//! phase's deadline every turn, so the wait follows the phase: kept while the
//! deadline is unchanged, re-armed for a new one, dropped the moment the phase
//! no longer carries one.

use crate::playback::{PlaybackClock, PlaybackSleep};
use chrono::{DateTime, Utc};

pub(super) struct SideCountdownWait {
    armed: Option<(DateTime<Utc>, PlaybackSleep)>,
}

impl SideCountdownWait {
    pub(super) fn new() -> Self {
        Self { armed: None }
    }

    /// Match the wait to the phase's current deadline.
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
