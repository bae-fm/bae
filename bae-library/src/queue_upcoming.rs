//! The context's upcoming tail, read through one live subscription whose
//! windows move as the queue scrolls.

use bae_playback::QueueUpcomingWindow;
use bae_store::LibraryPageWindows;

/// Every requested window as of one read. Offsets are into the
/// not-yet-played tail, the first entry after the playing one at offset 0, and
/// a window reaching past the tail's end holds what remains of it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingSnapshot {
    /// The length of the not-yet-played tail at the same read, so the windows
    /// and the length the UI lays them out in always agree.
    pub upcoming_total: u64,
    /// How many writes have changed the context lane's tracks after its
    /// playing one: a value at the version before it only moved its windows,
    /// as when the UI scrolls, and a screen holding a context row it moved
    /// shows the tables' order again once this reaches the version its move
    /// made.
    pub context_version: u64,
    pub windows: Vec<QueueUpcomingWindow>,
}

/// A live read of the upcoming tail's requested windows. The windows change
/// in place through [`super::LiveRead::set`], and a change to the queue reads
/// them again: neither opens another query.
pub type LiveQueueUpcoming = super::LiveRead<LibraryPageWindows, QueueUpcomingSnapshot>;

/// The context's upcoming tail, read in the windows the subscription is
/// asked for — none until the first [`LiveRead::set`] — with the tail's
/// length and the context lane's version. One query serves every window; a new window set
/// points it at the new windows.
///
/// [`LiveRead::set`]: super::LiveRead::set
pub fn subscribe_queue_upcoming(library: &crate::LibraryManager) -> crate::LiveQueueUpcoming {
    let (windows_tx, mut windows) =
        tokio::sync::watch::channel(bae_store::LibraryPageWindows::new());
    let (tx, rx) = crate::live_read_channel();
    let manager = library.clone();
    let task = crate::CancellationToken::new();
    library.spawn_stoppable_subscription_task(
        "queue upcoming",
        task.clone(),
        tx.clone(),
        async move {
            let mut query = crate::live_query_events::reconfigurable_live_query_events(
                &manager,
                manager.subscribe_queue_upcoming(windows.borrow_and_update().clone()),
                tx.clone(),
            );
            loop {
                tokio::select! {
                    event = query.recv() => {
                        let Some(result) = event else { return };
                        let value = result.map(|read| crate::QueueUpcomingSnapshot {
                            upcoming_total: read.upcoming_total,
                            context_version: read.context_version,
                            windows: read.windows,
                        });
                        if tx.send(value).is_err() { return; }
                    }
                    changed = windows.changed() => {
                        if changed.is_err() { return; }
                        query.set(windows.borrow_and_update().clone());
                    }
                }
            }
        },
    );
    crate::LiveQueueUpcoming::new(windows_tx, rx, task)
}
