//! The playback queue's live reads on [`AppServices`]: the queue value and the
//! upcoming tail's windows past it.

use super::*;

impl AppServices {
    pub fn subscribe_queue_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::queue::ResolvedQueueSnapshot, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let query_runtime = runtime_handle.clone();
        runtime_handle.spawn(async move {
            let manager = &services.inner.manager;
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let mut request = crate::library::manager::queue_catalog_request(&projection);
            let mut catalog = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_queue_catalog(request.clone()),
            );
            // The catalog last read for `request`, so a queue change that
            // shows the same tracks — reordered, say — resolves again
            // without another read.
            let mut current = None;
            loop {
                tokio::select! {
                    event = catalog.recv() => {
                        let Some(result) = event else { return };
                        let value = result.map(|read: crate::db::QueueCatalogProjection| {
                            current = Some(read.clone());
                            manager.resolve_queue_catalog(projection.clone(), read)
                        });
                        if tx.send(value).is_err() { return; }
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                        let next = crate::library::manager::queue_catalog_request(&projection);
                        if next != request {
                            request = next;
                            current = None;
                            catalog.set(request.clone());
                        } else if let Some(read) = current.clone() {
                            let value = manager.resolve_queue_catalog(projection.clone(), read);
                            if tx.send(Ok(value)).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }

    /// The context's upcoming tail, read in the windows the subscription is
    /// asked for — none until the first [`LiveRead::set`]. One catalog query
    /// serves every window: a new window set or a queue revision points it at
    /// the tracks now in those windows, and one that leaves those tracks
    /// alone resolves the read it already has.
    ///
    /// [`LiveRead::set`]: crate::library::LiveRead::set
    pub fn subscribe_queue_upcoming(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::QueueUpcomingSubscription {
        let (windows_tx, mut windows) =
            tokio::sync::watch::channel(crate::library::LibraryPageWindows::new());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let query_runtime = runtime_handle.clone();
        let task = runtime_handle.spawn(async move {
            let manager = &services.inner.manager;
            let mut queue_values = services.inner.playback.subscribe_queue_values();
            let mut projection = queue_values.borrow_and_update().clone();
            let mut requested = windows.borrow_and_update().clone();
            let catalog_request =
                |projection: &crate::playback::PlaybackQueueProjection,
                 requested: &crate::library::LibraryPageWindows| {
                    crate::db::QueueCatalogRequest::for_entries(
                        upcoming_slices(projection, requested)
                            .into_iter()
                            .flat_map(|(_, entries)| entries),
                        None,
                    )
                };
            let snapshot = |projection: &crate::playback::PlaybackQueueProjection,
                            requested: &crate::library::LibraryPageWindows,
                            read: &crate::db::QueueCatalogProjection| {
                crate::library::QueueUpcomingSnapshot {
                    revision: projection.revision,
                    windows: upcoming_slices(projection, requested)
                        .into_iter()
                        .map(|(window, entries)| crate::library::QueueUpcomingWindow {
                            window,
                            items: manager.resolve_queue_entries(read, entries),
                        })
                        .collect(),
                }
            };
            let mut request = catalog_request(&projection, &requested);
            let mut catalog = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_queue_catalog(request.clone()),
            );
            // The catalog last read for `request`: a queue revision or a
            // window change that shows the same tracks resolves it again
            // without another read.
            let mut current: Option<crate::db::QueueCatalogProjection> = None;
            loop {
                tokio::select! {
                    event = catalog.recv() => {
                        let Some(result) = event else { return };
                        let value = result.map(|read| {
                            let value = snapshot(&projection, &requested, &read);
                            current = Some(read);
                            value
                        });
                        if tx.send(value).is_err() { return; }
                        continue;
                    }
                    changed = queue_values.changed() => {
                        if changed.is_err() { return; }
                        projection = queue_values.borrow_and_update().clone();
                    }
                    changed = windows.changed() => {
                        if changed.is_err() { return; }
                        requested = windows.borrow_and_update().clone();
                    }
                }
                let next = catalog_request(&projection, &requested);
                if next != request {
                    request = next;
                    current = None;
                    catalog.set(request.clone());
                } else if let Some(read) = current.as_ref() {
                    if tx
                        .send(Ok(snapshot(&projection, &requested, read)))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        crate::library::QueueUpcomingSubscription::new(windows_tx, rx, task)
    }
}

/// Each requested window of `projection`'s upcoming tail with the entries in
/// it, clamped to the tail's end.
fn upcoming_slices<'a>(
    projection: &'a crate::playback::PlaybackQueueProjection,
    requested: &crate::library::LibraryPageWindows,
) -> Vec<(
    crate::library::LibraryPageWindow,
    &'a [crate::playback::QueueEntry],
)> {
    let tail = projection
        .context
        .as_ref()
        .map(|context| context.upcoming.as_slice())
        .unwrap_or(&[]);
    requested
        .iter()
        .map(|window| {
            (
                window.clone(),
                crate::queue::clamp_upcoming_page(tail, window.offset, window.limit),
            )
        })
        .collect()
}
