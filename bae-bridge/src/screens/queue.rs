//! The queue's upcoming tail as the queue screen pulls it.

use super::detail::live_read_error;
use crate::types::BridgeError;

/// One requested window of the context's upcoming tail, with its entries.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingWindow {
    pub window: crate::types::BridgeLibraryPageWindow,
    pub entries: Vec<crate::types::BridgeQueueEntry>,
}

/// Every requested window of the upcoming tail as of one read.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingSnapshot {
    /// The tail's length at the same read, which the windows are laid out in.
    pub upcoming_total: u64,
    /// How many writes have changed the context lane's tracks after its
    /// playing one; the same as the value before when only the windows moved.
    pub context_version: u64,
    pub windows: Vec<BridgeQueueUpcomingWindow>,
}

pulled_read! {
    /// The context's upcoming tail, read in the windows the UI asks for: one
    /// live read for as long as the queue is shown, moved in place as it
    /// scrolls and as the queue changes. It reads no windows until
    /// `set_windows`.
    QueueUpcomingSubscription: bae_library::LiveQueueUpcoming => BridgeQueueUpcomingSnapshot,
    converted by BridgeQueueUpcomingSnapshot::from_core,
    built by queue_upcoming
}

#[uniffi::export]
impl QueueUpcomingSubscription {
    /// Read `windows` of the upcoming tail from now on; offsets count from
    /// the first entry after the playing track.
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(
                windows.into_iter().map(crate::types::BridgeLibraryPageWindow::into_core).collect(),
            )
            .map_err(live_read_error)
    }
}

impl BridgeQueueUpcomingSnapshot {
    fn from_core(snapshot: bae_library::QueueUpcomingSnapshot) -> Self {
        Self {
            upcoming_total: snapshot.upcoming_total,
            context_version: snapshot.context_version,
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| BridgeQueueUpcomingWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    entries: window
                        .items
                        .into_iter()
                        .map(crate::types::BridgeQueueEntry::from_core)
                        .collect(),
                })
                .collect(),
        }
    }
}
