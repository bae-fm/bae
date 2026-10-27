//! The playback queue's live reads on [`AppServices`]: the queue value and the
//! upcoming tail's windows past it, both read from the queue tables.

use super::*;

impl AppServices {
    /// The queue as its screens show it, now and after every change to it.
    pub fn subscribe_queue_values(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::queue::ResolvedQueueSnapshot, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut query = self.inner.manager.subscribe_queue_value();
        self.inner
            .manager
            .spawn_subscription_task("queue values", async move {
                while let Some(value) = query.next().await {
                    if tx
                        .send(value.map_err(crate::library::LibraryError::from))
                        .is_err()
                    {
                        return;
                    }
                }
            });
        rx
    }

    /// The context's upcoming tail, read in the windows the subscription is
    /// asked for — none until the first [`LiveRead::set`]. One query serves
    /// every window; a new window set points it at the new windows.
    ///
    /// [`LiveRead::set`]: crate::library::LiveRead::set
    pub fn subscribe_queue_upcoming(&self) -> crate::library::QueueUpcomingSubscription {
        let (windows_tx, mut windows) =
            tokio::sync::watch::channel(crate::library::LibraryPageWindows::new());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "queue upcoming",
            task.clone(),
            async move {
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    manager.subscribe_queue_upcoming(windows.borrow_and_update().clone()),
                );
                loop {
                    tokio::select! {
                        event = query.recv() => {
                            let Some(result) = event else { return };
                            if tx.send(result).is_err() { return; }
                        }
                        changed = windows.changed() => {
                            if changed.is_err() { return; }
                            query.set(windows.borrow_and_update().clone());
                        }
                    }
                }
            },
        );
        crate::library::QueueUpcomingSubscription::new(windows_tx, rx, task)
    }
}
