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
    /// The queue's revision the windows were read at. A UI shows them only
    /// while its queue value carries the same revision.
    pub revision: u64,
    /// The length of the not-yet-played tail at the same read, so the windows
    /// and the length the UI lays them out in always agree.
    pub upcoming_total: u64,
    /// Whether the queue changed since the read before: `false` when only the
    /// windows moved, as when the UI scrolls.
    pub queue_changed: bool,
    pub windows: Vec<QueueUpcomingWindow>,
}

/// A live read of the upcoming tail's requested windows. The windows change
/// in place through [`super::LiveRead::set`], and a change to the queue reads
/// them again: neither opens another query.
pub type QueueUpcomingSubscription = super::LiveRead<LibraryPageWindows, QueueUpcomingSnapshot>;
