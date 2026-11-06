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
) -> bae_library::LiveDetail<bae_views::album_detail::AlbumDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_album_detail(id),
        bae_views::album::resolve_album_detail_projection,
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
) -> bae_library::LiveDetail<bae_views::album_detail::ReleaseDetail> {
    let views = library_views.clone();
    follow_request(
        library,
        "detail",
        move |id| views.subscribe_release_detail(id),
        |id, projection| bae_library::DetailSnapshot {
            id: id.clone(),
            value: id.as_ref().and_then(|release_id| {
                bae_views::release::resolve_release_detail_projection(release_id, projection)
            }),
        },
    )
}

/// The artist detail view's artist as it changes; another artist is a
/// new id on the same read.
pub fn subscribe_artist_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> bae_library::LiveDetail<bae_views::album_detail::ArtistDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_artist_detail(id),
        bae_views::artist::resolve_artist_detail_projection,
    )
}

/// The composer detail view's composer and the work its page opens first,
/// as they change; another composer is a new id on the same read. The page's
/// rows are a list of their own (`AppServices::subscribe_composer_detail_rows`).
pub fn subscribe_composer_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> bae_library::LiveDetail<bae_views::album_detail::ComposerDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_composer_detail(id),
        bae_views::composer_detail::resolve_composer_detail_projection,
    )
}

/// The work detail view's work as it changes; another work is a new id
/// on the same read.
pub fn subscribe_work_detail(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> bae_library::LiveDetail<bae_views::album_detail::WorkDetail> {
    let views = library_views.clone();
    detail_read(
        library.clone(),
        move |id| views.subscribe_work_detail(id),
        bae_views::composer::resolve_work_detail_projection,
    )
}

/// A detail read with nothing to merge: the query `open` makes for the id
/// set first, each value resolved for the id it was read for. A new id points
/// the same query at it.
pub fn detail_read<Projection, Value, Id>(
    manager: LibraryManager,
    open: impl FnOnce(
            Option<Id>,
        ) -> coven::ReconfigurableLiveQuery<Option<Id>, Projection, coven::CovenError>
        + Send
        + 'static,
    resolve: impl Fn(Projection) -> Option<Value> + Send + 'static,
) -> bae_library::LiveDetail<Value, Id>
where
    Projection: Clone + PartialEq + Send + 'static,
    Value: Send + 'static,
    Id: Clone + PartialEq + std::fmt::Debug + Send + Sync + 'static,
{
    follow_request(&manager, "detail", open, move |id, projection| bae_library::DetailSnapshot {
        id: id.clone(),
        value: resolve(projection),
    })
}
