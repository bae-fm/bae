//! The detail views' live reads on [`AppServices`]: one read per view, its
//! id moved in place as the view shows another album, release, artist,
//! composer, or work.

use super::*;

impl AppServices {
    /// The detail view's album as it changes, read for the id the
    /// subscription is set to: its rows, the releases' pin markers coven
    /// watches, whether a cloud home is connected, and the transfers of its
    /// releases — the state it is resolved against. Another album is a new id
    /// on the same read.
    pub fn subscribe_album_detail(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<crate::album_detail::AlbumDetail> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        let mut pins = manager.watch_release_pins();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut transfers = ShownTransfers::new(services.subscribe_transfer_values());
        let task = runtime_handle.spawn(async move {
            let mut id = ids.borrow_and_update().clone();
            let mut query = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_album_detail(id.clone()),
            );
            let mut last: Option<(crate::db::AlbumDetailProjection, Vec<bool>)> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::album_detail_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    transfers.show(LibraryManager::album_detail_release_ids(&projection));
                                    last = Some((projection.clone(), pinned.clone()));
                                    manager.resolve_album_detail_projection(projection, pinned)
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.clone();
                            manager.resolve_album_detail_projection(projection.clone(), pinned)
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = ids.changed() => {
                        if changed.is_err() { return; }
                        id = ids.borrow_and_update().clone();
                        last = None;
                        query.set(id.clone());
                        continue;
                    }
                    changed = async { tokio::select! {
                        value = cloud_home.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        cloud_home.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        manager.resolve_album_detail_projection(projection, pinned)
                    }
                };
                let value = value.map(|value| crate::library::DetailSnapshot {
                    id: id.clone(),
                    value,
                });
                if tx.send(value).is_err() { return; }
            }
        });
        crate::library::DetailSubscription::new(id_tx, rx, task)
    }

    /// The detail view's release as it changes, read for the id the
    /// subscription is set to: its rows, its pin marker coven watches, whether
    /// a cloud home is connected, and its transfer — the state it is resolved
    /// against. Another release is a new id on the same read.
    pub fn subscribe_release_detail(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<crate::album_detail::ReleaseDetail> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = services.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        let mut pins = manager.watch_release_pins();
        let mut cloud_home = manager.subscribe_cloud_home();
        let mut transfers = ShownTransfers::new(services.subscribe_transfer_values());
        let task = runtime_handle.spawn(async move {
            let mut id = ids.borrow_and_update().clone();
            let mut query = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_release_detail(id.clone()),
            );
            let mut last: Option<(crate::db::ReleaseDetailProjection, bool)> = None;
            let resolve = |id: &Option<String>, projection, pinned| match id {
                Some(release_id) => {
                    manager.resolve_release_detail_projection(release_id, projection, pinned)
                }
                None => Ok(None),
            };
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(projection)) => {
                            match pins.watch(LibraryManager::release_detail_pin_files(&projection)).await {
                                Ok(pinned) => {
                                    let pinned = pinned.first().copied().unwrap_or(false);
                                    transfers.show(id.iter().cloned().collect());
                                    last = Some((projection.clone(), pinned));
                                    resolve(&id, projection, pinned)
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Some(Err(error)) => Err(error),
                    },
                    answer = pins.changed() => match (answer, last.as_mut()) {
                        (Ok(pinned), Some((projection, last_pinned))) => {
                            *last_pinned = pinned.first().copied().unwrap_or(false);
                            resolve(&id, projection.clone(), *last_pinned)
                        }
                        (Ok(_), None) => continue,
                        (Err(error), _) => Err(error),
                    },
                    changed = ids.changed() => {
                        if changed.is_err() { return; }
                        id = ids.borrow_and_update().clone();
                        last = None;
                        query.set(id.clone());
                        continue;
                    }
                    changed = async { tokio::select! {
                        value = cloud_home.changed() => value,
                        value = transfers.changed() => value,
                    }} => {
                        if changed.is_err() { return; }
                        cloud_home.borrow_and_update();
                        let Some((projection, pinned)) = last.clone() else { continue };
                        resolve(&id, projection, pinned)
                    }
                };
                let value = value.map(|value| crate::library::DetailSnapshot {
                    id: id.clone(),
                    value,
                });
                if tx.send(value).is_err() { return; }
            }
        });
        crate::library::DetailSubscription::new(id_tx, rx, task)
    }

    /// The artist detail view's artist as it changes; another artist is a
    /// new id on the same read.
    pub fn subscribe_artist_detail(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<crate::album_detail::ArtistDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            runtime_handle,
            {
                let manager = manager.clone();
                move |id| manager.subscribe_artist_detail(id)
            },
            move |projection| manager.resolve_artist_detail_projection(projection),
        )
    }

    /// The composer detail view's composer as it changes; another composer
    /// is a new id on the same read.
    pub fn subscribe_composer_detail(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<crate::album_detail::ComposerDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            runtime_handle,
            {
                let manager = manager.clone();
                move |id| manager.subscribe_composer_detail(id)
            },
            move |projection| manager.resolve_composer_detail_projection(projection),
        )
    }

    /// The work detail view's work as it changes; another work is a new id
    /// on the same read.
    pub fn subscribe_work_detail(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<crate::album_detail::WorkDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            runtime_handle,
            {
                let manager = manager.clone();
                move |id| manager.subscribe_work_detail(id)
            },
            move |projection| manager.resolve_work_detail_projection(projection),
        )
    }
}

/// A detail read with nothing to merge: the query `open` makes for the id
/// set first, each value resolved for the id it was read for. A new id points
/// the same query at it.
fn detail_read<Projection, Value>(
    runtime_handle: &tokio::runtime::Handle,
    open: impl FnOnce(Option<String>) -> coven::ReconfigurableLiveQuery<Option<String>, Projection>
        + Send
        + 'static,
    resolve: impl Fn(Projection) -> Option<Value> + Send + 'static,
) -> crate::library::DetailSubscription<Value>
where
    Projection: Clone + PartialEq + Send + 'static,
    Value: Send + 'static,
{
    let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let query_runtime = runtime_handle.clone();
    let task = runtime_handle.spawn(async move {
        let mut id = ids.borrow_and_update().clone();
        let mut query = reconfigurable_live_query_events(&query_runtime, open(id.clone()));
        loop {
            tokio::select! {
                event = query.recv() => {
                    let Some(result) = event else { return };
                    let value = result.map(|projection| crate::library::DetailSnapshot {
                        id: id.clone(),
                        value: resolve(projection),
                    });
                    if tx.send(value).is_err() {
                        return;
                    }
                }
                changed = ids.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    id = ids.borrow_and_update().clone();
                    query.set(id.clone());
                }
            }
        }
    });
    crate::library::DetailSubscription::new(id_tx, rx, task)
}
