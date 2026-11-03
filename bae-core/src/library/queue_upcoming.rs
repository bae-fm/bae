//! The context's upcoming tail, read through one live subscription whose
//! windows move as the queue scrolls.

use bae_db::LibraryPageWindows;
use bae_playback::QueueUpcomingWindow;

/// Every requested window as of one read. Offsets are into the
/// not-yet-played tail, the first entry after the playing one at offset 0, and
/// a window reaching past the tail's end holds what remains of it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingSnapshot {
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
pub type LiveQueueUpcoming = super::LiveRead<LibraryPageWindows, QueueUpcomingSnapshot>;
