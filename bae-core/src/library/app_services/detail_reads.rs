//! The detail views' live reads on [`AppServices`]: one read per view, its
//! id moved in place as the view shows another album, release, artist,
//! composer, work, or folder that failed import validation.

use super::*;

impl AppServices {
    /// The detail view's album as it changes, read for the id the
    /// subscription is set to: its rows, with each release's pin marker,
    /// download-queue entry and storage transition, and whether a cloud home
    /// is connected — the state it is resolved against. Another album is a
    /// new id on the same read.
    pub fn subscribe_album_detail(
        &self,
    ) -> crate::library::DetailSubscription<crate::album_detail::AlbumDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            manager.clone(),
            {
                let manager = manager.clone();
                move |id| manager.subscribe_album_detail(id)
            },
            move |projection| manager.resolve_album_detail_projection(projection),
        )
    }

    /// The detail view's release as it changes, read for the id the
    /// subscription is set to: its rows, with its pin marker, download-queue
    /// entry and storage transition, and whether a cloud home is connected —
    /// the state it is resolved against. Another release is a new id on the
    /// same read.
    pub fn subscribe_release_detail(
        &self,
    ) -> crate::library::DetailSubscription<crate::album_detail::ReleaseDetail> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = crate::library::live_read_channel();
        let manager = self.inner.manager.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "detail",
            task.clone(),
            tx.clone(),
            async move {
                let mut id = ids.borrow_and_update().clone();
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    manager.subscribe_release_detail(id.clone()),
                    tx.clone(),
                );
                let resolve = |id: &Option<String>, projection| {
                    Ok(id.as_ref().and_then(|release_id| {
                        manager.resolve_release_detail_projection(release_id, projection)
                    }))
                };
                loop {
                    let value = tokio::select! {
                        event = query.recv() => match event {
                            None => return,
                            Some(Ok(projection)) => {
                                resolve(&id, projection)
                            }
                            Some(Err(error)) => Err(error),
                        },
                        changed = ids.changed() => {
                            if changed.is_err() { return; }
                            id = ids.borrow_and_update().clone();
                            query.set(id.clone());
                            continue;
                        }
                    };
                    let value =
                        value.map(|value| crate::library::DetailSnapshot { id: id.clone(), value });
                    if tx.send(value).is_err() {
                        return;
                    }
                }
            },
        );
        crate::library::DetailSubscription::new(id_tx, rx, task)
    }

    /// The artist detail view's artist as it changes; another artist is a
    /// new id on the same read.
    pub fn subscribe_artist_detail(
        &self,
    ) -> crate::library::DetailSubscription<crate::album_detail::ArtistDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            manager.clone(),
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
    ) -> crate::library::DetailSubscription<crate::album_detail::ComposerDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            manager.clone(),
            {
                let manager = manager.clone();
                move |id| manager.subscribe_composer_detail(id)
            },
            move |projection| manager.resolve_composer_detail_projection(projection),
        )
    }

    /// The folder that failed validation the import pane looks into, as it
    /// changes; another folder is a new key on the same read.
    #[cfg(desktop)]
    pub fn subscribe_invalid_candidate(
        &self,
    ) -> crate::library::DetailSubscription<
        crate::import::InvalidCandidateDetail,
        crate::import::CandidateKey,
    > {
        let manager = self.inner.manager.clone();
        detail_read(
            manager.clone(),
            move |key| manager.subscribe_invalid_candidate(key),
            |detail| detail,
        )
    }

    /// The work detail view's work as it changes; another work is a new id
    /// on the same read.
    pub fn subscribe_work_detail(
        &self,
    ) -> crate::library::DetailSubscription<crate::album_detail::WorkDetail> {
        let manager = self.inner.manager.clone();
        detail_read(
            manager.clone(),
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
fn detail_read<Projection, Value, Id>(
    manager: LibraryManager,
    open: impl FnOnce(Option<Id>) -> coven::ReconfigurableLiveQuery<Option<Id>, Projection>
        + Send
        + 'static,
    resolve: impl Fn(Projection) -> Option<Value> + Send + 'static,
) -> crate::library::DetailSubscription<Value, Id>
where
    Projection: Clone + PartialEq + Send + 'static,
    Value: Send + 'static,
    Id: Clone + PartialEq + std::fmt::Debug + Send + Sync + 'static,
{
    let (id_tx, mut ids) = tokio::sync::watch::channel(None::<Id>);
    let (tx, rx) = crate::library::live_read_channel();
    let task = crate::library::CancellationToken::new();
    manager.clone().spawn_stoppable_subscription_task(
        "detail",
        task.clone(),
        tx.clone(),
        async move {
            let mut id = ids.borrow_and_update().clone();
            let mut query =
                reconfigurable_live_query_events(&manager, open(id.clone()), tx.clone());
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
        },
    );
    crate::library::DetailSubscription::new(id_tx, rx, task)
}
