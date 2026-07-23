//! The Subsonic endpoint handlers, grouped by concern, plus the shared
//! request/response glue.

use axum::handler::Handler;
use axum::routing::get;
use axum::Router;

use crate::envelope::{error_response, ok_response, Element, Format};
use crate::error::SubError;
use crate::AppState;

pub(crate) mod browse;
pub(crate) mod lists;
pub(crate) mod media;
pub(crate) mod system;

/// Mount every endpoint under both `/<name>` and `/<name>.view`.
pub(crate) fn mount() -> Router<AppState> {
    let mut router = Router::new();
    // system
    router = dual(router, "ping", system::ping);
    router = dual(router, "getLicense", system::get_license);
    router = dual(router, "getMusicFolders", system::get_music_folders);
    // browse
    router = dual(router, "getArtists", browse::get_artists);
    router = dual(router, "getIndexes", browse::get_indexes);
    router = dual(router, "getArtist", browse::get_artist);
    router = dual(router, "getAlbum", browse::get_album);
    // lists
    router = dual(router, "getAlbumList2", lists::get_album_list2);
    router = dual(router, "getSong", lists::get_song);
    router = dual(router, "search3", lists::search3);
    // media
    router = dual(router, "stream", media::stream);
    router = dual(router, "getCoverArt", media::get_cover_art);
    router = dual(router, "scrobble", media::scrobble);
    router
}

/// Register `handler` at both `/<name>` and `/<name>.view`. Function-item
/// handlers are `Copy`, so the same handler mounts at both paths.
fn dual<H, T>(router: Router<AppState>, name: &str, handler: H) -> Router<AppState>
where
    H: Handler<T, AppState> + Copy,
    T: 'static,
{
    router
        .route(&format!("/{name}"), get(handler))
        .route(&format!("/{name}.view"), get(handler))
}

/// Render an endpoint's outcome as a Subsonic envelope in `format`: the payload
/// element on success (or an empty ok envelope when there is none), the error
/// envelope on failure.
pub(crate) fn respond(
    format: &Format,
    result: Result<Option<Element>, SubError>,
) -> axum::response::Response {
    match result {
        Ok(payload) => ok_response(format, payload),
        Err(error) => error_response(format, &error),
    }
}
