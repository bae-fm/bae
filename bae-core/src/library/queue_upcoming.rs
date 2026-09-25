//! The context's upcoming tail past the queue value's own first window, read
//! through one live subscription whose windows move as the queue scrolls.

use super::{LibraryPageWindow, LibraryPageWindows};

/// One requested window of the upcoming tail, with the entries in it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingWindow {
    pub window: LibraryPageWindow,
    pub items: Vec<crate::queue::QueueItem>,
}

/// Every requested window as of one queue revision. Offsets are into the
/// not-yet-played tail — the same coordinate space as
/// [`crate::queue::ResolvedContext::upcoming`] — and a window reaching past
/// the tail's end holds what remains of it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingSnapshot {
    /// The `PlaybackQueue` revision the windows were sliced from. A UI shows
    /// them only while its queue value carries the same revision.
    pub revision: u64,
    pub windows: Vec<QueueUpcomingWindow>,
}

/// A live read of the upcoming tail's requested windows. The windows change
/// in place through [`super::LiveRead::set`] and the queue's own revisions move the
/// same read: neither opens another query.
pub type QueueUpcomingSubscription = super::LiveRead<LibraryPageWindows, QueueUpcomingSnapshot>;
