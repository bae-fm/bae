//! The queue's upcoming tail as the queue screen pulls it.

use crate::types::BridgeError;

/// One requested window of the context's upcoming tail, with its entries.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingWindow {
    pub window: crate::types::BridgeQueueWindow,
    pub rows: Vec<crate::types::BridgeQueueEntry>,
}

impl BridgeQueueUpcomingWindow {
    fn from_core(read: bae_playback::QueueWindowRead<bae_playback::QueueItem>) -> Self {
        Self {
            window: crate::types::BridgeQueueWindow {
                offset: read.window.offset,
                limit: read.window.limit,
            },
            rows: read.rows.into_iter().map(crate::types::BridgeQueueEntry::from_core).collect(),
        }
    }
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
        windows: Vec<crate::types::BridgeQueueWindow>,
    ) -> Result<(), BridgeError> {
        let windows = windows
            .into_iter()
            .map(|window| bae_playback::QueueWindow::at(window.offset, window.limit))
            .collect();
        self.inner.set(windows).map(|_revision| ()).map_err(BridgeError::from)
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
                .map(BridgeQueueUpcomingWindow::from_core)
                .collect(),
        }
    }
}
