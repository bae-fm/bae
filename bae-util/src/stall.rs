//! A stall's reports: a wait that goes on — a reader waiting for bytes, an
//! output starved of samples, a lock the audio callback misses — reported
//! once it has lasted a while, again at most once a second while it lasts,
//! and when it ends, so a long stall is seen and a short one is not.

use std::time::{Duration, Instant};

/// How long a stall lasts before it is reported.
pub const REPORT_AFTER: Duration = Duration::from_millis(250);
/// The least time between two reports of one stall.
pub const REPORT_EVERY: Duration = Duration::from_secs(1);

/// One stall, while it lasts: when it began and when it was last reported.
#[derive(Debug, Clone, Copy)]
struct Stalled {
    started: Instant,
    last_report: Option<Instant>,
}

/// A wait that may stall, and whether one is in progress.
#[derive(Debug, Default)]
pub struct Stall {
    stalled: Option<Stalled>,
}

impl Stall {
    /// The wait goes on at `now`, starting a stall when none is in
    /// progress. How long the stall has lasted when it is time to report
    /// it: it has lasted [`REPORT_AFTER`], and it was not reported in the
    /// last [`REPORT_EVERY`].
    pub fn waiting(&mut self, now: Instant) -> Option<Duration> {
        let stalled = self.stalled.get_or_insert(Stalled { started: now, last_report: None });
        let lasted = now.duration_since(stalled.started);
        let due = lasted >= REPORT_AFTER
            && stalled.last_report.is_none_or(|last| now.duration_since(last) >= REPORT_EVERY);
        if due {
            stalled.last_report = Some(now);
        }
        due.then_some(lasted)
    }

    /// The wait ended at `now`. How long the stall lasted, when one was in
    /// progress and lasted [`REPORT_AFTER`].
    pub fn ended(&mut self, now: Instant) -> Option<Duration> {
        let stalled = self.stalled.take()?;
        let lasted = now.duration_since(stalled.started);
        (lasted >= REPORT_AFTER).then_some(lasted)
    }

    /// The wait is over with nothing to report, such as a source that
    /// finished.
    pub fn clear(&mut self) {
        self.stalled = None;
    }
}

#[cfg(test)]
#[path = "stall_tests.rs"]
mod tests;
