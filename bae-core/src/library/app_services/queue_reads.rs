//! The playback queue's live reads on [`AppServices`]: the queue value, the
//! upcoming tail's windows past it, and the last addition to Up Next, all
//! read from the queue tables.

use super::*;

impl AppServices {
    /// The queue as its screens show it: the first value at once, then one
    /// after each write that changes it.
    pub fn subscribe_queue_values(
        &self,
    ) -> crate::live_query::LiveValues<bae_playback::ResolvedQueueSnapshot> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_queue_value())
    }

    /// The last time tracks went into Up Next, as the screens confirming
    /// each addition read it: the first value at once, then one per
    /// addition. `None` until there is one since playback started with the
    /// library.
    pub fn subscribe_queue_additions(
        &self,
    ) -> crate::live_query::LiveValues<Option<bae_playback::QueueAddition>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_queue_addition())
    }

    /// The context's upcoming tail, read in the windows the subscription is
    /// asked for — none until the first [`LiveRead::set`] — with the tail's
    /// length, and whether the queue changed since the value before or only
    /// the windows moved. One query serves every window; a new window set
    /// points it at the new windows.
    ///
    /// [`LiveRead::set`]: crate::library::LiveRead::set
    pub fn subscribe_queue_upcoming(&self) -> crate::library::LiveQueueUpcoming {
        let (windows_tx, mut windows) =
            tokio::sync::watch::channel(bae_db::LibraryPageWindows::new());
        let (tx, rx) = crate::library::live_read_channel();
        let manager = self.inner.manager.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "queue upcoming",
            task.clone(),
            tx.clone(),
            async move {
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    manager.subscribe_queue_upcoming(windows.borrow_and_update().clone()),
                    tx.clone(),
                );
                loop {
                    tokio::select! {
                        event = query.recv_with_cause() => {
                            let Some((cause, result)) = event else { return };
                            let value = result.map(|read| crate::library::QueueUpcomingSnapshot {
                                upcoming_total: read.upcoming_total,
                                queue_changed: !matches!(cause, coven::LiveQueryCause::Request),
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
        crate::library::LiveQueueUpcoming::new(windows_tx, rx, task)
    }
}
