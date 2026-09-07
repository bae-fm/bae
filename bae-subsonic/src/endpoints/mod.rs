//! The Subsonic endpoint handlers, grouped by concern, plus the shared
//! request/response glue.

use std::collections::HashMap;
use std::future::Future;

use axum::extract::{Query, State};
use axum::handler::Handler;
use axum::routing::get;
use axum::Router;

use crate::envelope::{error_response, ok_response, Element};
use crate::error::SubError;
use crate::params::Params;
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
    router = dual_raw(router, "stream", media::stream);
    router = dual_raw(router, "getCoverArt", media::get_cover_art);
    router = dual(router, "scrobble", media::scrobble);
    router
}

/// Register `endpoint` at both `/<name>` and `/<name>.view`, wrapped in the
/// request/response shell every envelope endpoint shares: read the query
/// parameters, resolve the response format, and render the outcome as an
/// envelope — so an endpoint is only its own `(state, params) -> payload`.
fn dual<F, Fut>(router: Router<AppState>, name: &str, endpoint: F) -> Router<AppState>
where
    F: Fn(AppState, Params) -> Fut + Copy + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<Element>, SubError>> + Send + 'static,
{
    dual_raw(
        router,
        name,
        move |State(state): State<AppState>, Query(query): Query<HashMap<String, String>>| async move {
            let params = Params(query);
            let format = params.format();
            match endpoint(state, params).await {
                Ok(payload) => ok_response(&format, payload),
                Err(error) => error_response(&format, &error),
            }
        },
    )
}

/// Register an endpoint that builds its own response — one serving media rather
/// than an envelope. Function-item and non-capturing handlers are `Copy`, so the
/// same handler mounts at both paths.
fn dual_raw<H, T>(router: Router<AppState>, name: &str, handler: H) -> Router<AppState>
where
    H: Handler<T, AppState> + Copy,
    T: 'static,
{
    router
        .route(&format!("/{name}"), get(handler))
        .route(&format!("/{name}.view"), get(handler))
}
