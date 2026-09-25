//! The Storage Manager's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// The Storage Manager list as it changes, read in the view the
    /// subscription is asked for: its rows, the rows' pin markers coven
    /// watches, the upload queue (which the Uploading filter lists, in queue
    /// order), and the config, cloud-home, download, and transfer state the
    /// rows are resolved against. One query serves every window: a new view or
    /// upload queue points it at what that reads.
    pub fn subscribe_storage_browse(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        initial: crate::library::StorageBrowseView,
    ) -> crate::library::StorageBrowseSubscription {
        let (view_tx, mut views) = tokio::sync::watch::channel(initial);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        let mut outbox = services.subscribe_outbox_values();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut config = services.subscribe_config_changes();
        let mut downloads = services.subscribe_download_values();
        let mut transfers = services.subscribe_transfer_values();
        let mut pins = manager.watch_release_pins();
        let task = runtime_handle.spawn(async move {
            // The upload queue as the outbox holds it now, read only while
            // the Uploading filter is shown.
            let upload_queue = |outbox: &mut tokio::sync::watch::Receiver<
                Option<Result<crate::library::OutboxSnapshot, String>>,
            >| {
                let current = outbox.borrow_and_update().clone();
                let services = services.clone();
                async move {
                    match current {
                        Some(Ok(snapshot)) => Ok(snapshot.transitioning_release_ids()),
                        Some(Err(error)) => Err(crate::library::LibraryError::Internal(error)),
                        None => services
                            .outbox_snapshot()
                            .await
                            .map(|snapshot| snapshot.transitioning_release_ids()),
                    }
                    .map(upload_queue_order)
                }
            };
            let mut view = views.borrow_and_update().clone();
            let mut uploading = Vec::new();
            if view.filter == crate::db::StorageFilter::Uploading {
                match upload_queue(&mut outbox).await {
                    Ok(ids) => uploading = ids,
                    Err(error) => {
                        let _ = tx.send(Err(error));
                        return;
                    }
                }
            }
            let request_for = |view: &crate::library::StorageBrowseView, uploading: &[String]| {
                crate::db::StorageBrowseRequest {
                    sort: view.sort,
                    filter: view.filter,
                    uploading: uploading.to_vec(),
                    windows: view.windows.clone(),
                }
            };
            let mut request = request_for(&view, &uploading);
            let mut query = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_storage_browse(request.clone()),
            );
            let mut last: Option<(crate::db::StorageBrowseProjection, Vec<bool>)> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::storage_browse_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    last = Some((projection.clone(), pinned.clone()));
                                    Ok(manager.resolve_storage_browse(projection, pinned))
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.clone();
                            Ok(manager.resolve_storage_browse(projection.clone(), pinned))
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = views.changed() => {
                        if changed.is_err() { return; }
                        view = views.borrow_and_update().clone();
                        uploading = if view.filter == crate::db::StorageFilter::Uploading {
                            match upload_queue(&mut outbox).await {
                                Ok(ids) => ids,
                                Err(error) => {
                                    if tx.send(Err(error)).is_err() { return; }
                                    continue;
                                }
                            }
                        } else {
                            Vec::new()
                        };
                        let next = request_for(&view, &uploading);
                        if next != request {
                            request = next;
                            last = None;
                            query.set(request.clone());
                        }
                        continue;
                    }
                    changed = outbox.changed(), if view.filter == crate::db::StorageFilter::Uploading => {
                        if changed.is_err() { return; }
                        match outbox.borrow_and_update().clone() {
                            // Byte progress leaves the queue as it was, and
                            // reads nothing again.
                            Some(Ok(snapshot)) => {
                                uploading = upload_queue_order(snapshot.transitioning_release_ids());
                                let next = request_for(&view, &uploading);
                                if next != request {
                                    request = next;
                                    last = None;
                                    query.set(request.clone());
                                }
                                continue;
                            }
                            Some(Err(error)) => Err(crate::library::LibraryError::Internal(error)),
                            None => continue,
                        }
                    }
                    changed = async { tokio::select! { value = cloud_home.changed() => value, value = config.changed() => value, value = downloads.changed() => value, value = transfers.changed() => value } } => {
                        if changed.is_err() { return; }
                        cloud_home.borrow_and_update(); config.borrow_and_update(); downloads.borrow_and_update(); transfers.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        Ok(manager.resolve_storage_browse(projection, pinned))
                    }
                };
                if tx.send(value).is_err() {
                    return;
                }
            }
        });
        crate::library::StorageBrowseSubscription::new(view_tx, rx, task)
    }
}

/// The releases the upload queue holds, once each, in queue order — the
/// order the Storage Manager's Uploading filter lists them in.
fn upload_queue_order(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_upload_queue_keeps_its_order_and_names_each_release_once() {
        assert_eq!(
            upload_queue_order(vec![
                "release-b".to_string(),
                "release-a".to_string(),
                "release-b".to_string(),
            ]),
            ["release-b", "release-a"],
            "queue order is what the Uploading filter lists by, so it is kept"
        );
    }
}
