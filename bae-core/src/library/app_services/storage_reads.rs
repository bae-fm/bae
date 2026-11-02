//! The Storage Manager's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// The Storage Manager list as it changes, read in the view the
    /// subscription is asked for: its rows, whose pin markers the query reads
    /// from the listing's pinned counts, whose uploads from coven's upload
    /// queue and whose transitions from `release_storage_action` and the
    /// download queue, and whether a cloud home is connected — the state the
    /// rows are resolved against. One query serves every window: a new view
    /// points it at what that reads.
    pub fn subscribe_storage_browse(
        &self,
        initial: crate::library::StorageBrowseView,
    ) -> crate::library::LiveStorageBrowse {
        let (view_tx, mut views) = tokio::sync::watch::channel(initial);
        let (tx, rx) = crate::library::live_read_channel();
        let manager = self.inner.manager.clone();
        let library_views = self.inner.views.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "storage browse",
            task.clone(),
            tx.clone(),
            async move {
                let request_for = |view: &crate::library::StorageBrowseView| {
                    crate::library::views::StorageBrowseRequest {
                        sort: view.sort,
                        filter: view.filter,
                        search: view.search.clone(),
                        windows: view.windows.clone(),
                        selected: view.selected.clone(),
                    }
                };
                let mut request = request_for(&views.borrow_and_update());
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    library_views.subscribe_storage_browse(request.clone()),
                    tx.clone(),
                );
                loop {
                    let value = tokio::select! {
                        event = query.recv() => match event {
                            None => return,
                            Some(Ok(projection)) => {
                                Ok(crate::library::views::storage::resolve_storage_browse(
                                    projection,
                                ))
                            }
                            Some(Err(error)) => Err(error),
                        },
                        changed = views.changed() => {
                            if changed.is_err() { return; }
                            let next = request_for(&views.borrow_and_update());
                            if next != request {
                                request = next;
                                query.set(request.clone());
                            }
                            continue;
                        }
                    };
                    if tx.send(value).is_err() {
                        return;
                    }
                }
            },
        );
        crate::library::LiveStorageBrowse::new(view_tx, rx, task)
    }
}
