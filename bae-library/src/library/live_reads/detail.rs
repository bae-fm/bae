//! The detail views' live reads on [`AppServices`]: one read per view, its
//! id moved in place as the view shows another album, release, artist,
//! composer, work, or folder that failed import validation.

use super::*;

/// The detail view's album as it changes, read for the id the
/// subscription is set to: its rows, with each release's pin marker,
/// download-queue entry and storage transition, and whether a cloud home
/// is connected — the state it is resolved against. Another album is a
/// new id on the same read.
pub fn subscribe_album_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> crate::library::LiveDetail<crate::album_detail::AlbumDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_album_detail(id),
        crate::library::views::album::resolve_album_detail_projection,
    )
}

/// The detail view's release as it changes, read for the id the
/// subscription is set to: its rows, with its pin marker, download-queue
/// entry and storage transition, and whether a cloud home is connected —
/// the state it is resolved against. Another release is a new id on the
/// same read.
pub fn subscribe_release_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> crate::library::LiveDetail<crate::album_detail::ReleaseDetail> {
    let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
    let (tx, rx) = crate::library::live_read_channel();
    let manager = library.clone();
    let views = library_views.clone();
    let task = crate::library::CancellationToken::new();
    library.spawn_stoppable_subscription_task("detail", task.clone(), tx.clone(), async move {
        let mut id = ids.borrow_and_update().clone();
        let mut query = reconfigurable_live_query_events(
            &manager,
            views.subscribe_release_detail(id.clone()),
            tx.clone(),
        );
        let resolve = |id: &Option<String>, projection| {
            Ok(id.as_ref().and_then(|release_id| {
                crate::library::views::release::resolve_release_detail_projection(
                    release_id, projection,
                )
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
            let value = value.map(|value| crate::library::DetailSnapshot { id: id.clone(), value });
            if tx.send(value).is_err() {
                return;
            }
        }
    });
    crate::library::LiveDetail::new(id_tx, rx, task)
}

/// The artist detail view's artist as it changes; another artist is a
/// new id on the same read.
pub fn subscribe_artist_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> crate::library::LiveDetail<crate::album_detail::ArtistDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_artist_detail(id),
        crate::library::views::artist::resolve_artist_detail_projection,
    )
}

/// The composer detail view's composer as it changes; another composer
/// is a new id on the same read.
pub fn subscribe_composer_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> crate::library::LiveDetail<crate::album_detail::ComposerDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_composer_detail(id),
        crate::library::views::composer::resolve_composer_detail_projection,
    )
}

/// The work detail view's work as it changes; another work is a new id
/// on the same read.
pub fn subscribe_work_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> crate::library::LiveDetail<crate::album_detail::WorkDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_work_detail(id),
        crate::library::views::composer::resolve_work_detail_projection,
    )
}

/// A detail read with nothing to merge: the query `open` makes for the id
/// set first, each value resolved for the id it was read for. A new id points
/// the same query at it.
pub fn detail_read<Projection, Value, Id>(
    manager: LibraryManager,
    open: impl FnOnce(Option<Id>) -> coven::ReconfigurableLiveQuery<Option<Id>, Projection>
        + Send
        + 'static,
    resolve: impl Fn(Projection) -> Option<Value> + Send + 'static,
) -> crate::library::LiveDetail<Value, Id>
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
    crate::library::LiveDetail::new(id_tx, rx, task)
}
