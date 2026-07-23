//! A Subsonic/OpenSubsonic REST server over a bae library.
//!
//! Exposes bae's library for browse and play to third-party Subsonic clients
//! (Symfonium, Amperfy, play:Sub, Feishin, …), reaching platforms bae ships no
//! native client to. The scope is the browse+play core; everything else
//! (playlists, star/rating, jukebox, podcasts, downloads, …) is out of scope.
//!
//! The desktop apps drive the server through [`SubsonicServerController`], which
//! owns its lifecycle the way `bae-mcp`'s `McpServerController` does. The
//! low-level entries stay public too: [`router`] builds the axum router (the
//! seam integration tests drive), and [`serve`] binds an address and runs it.
//! Nothing in `bae-core` depends on this crate.
//!
//! Implemented from the Subsonic API doc (subsonic.org) and the OpenSubsonic
//! spec (opensubsonic.netlify.app) only.
#![deny(unreachable_pub, dead_code)]

use std::sync::Arc;

use axum::extract::FromRef;
use axum::Router;
use bae_core::config::SubsonicCredential;
use bae_core::library::LibraryManager;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::warn;

mod auth;
mod controller;
mod endpoints;
mod envelope;
mod error;
mod id;
mod library_map;
mod model;
mod params;

pub use controller::{SubsonicServerController, SubsonicServerError, SubsonicServerStatus};

/// Shared handler state: the library and the one accepted credential.
#[derive(Clone)]
pub(crate) struct AppState {
    manager: LibraryManager,
    credential: Arc<SubsonicCredential>,
}

impl FromRef<AppState> for Arc<SubsonicCredential> {
    fn from_ref(state: &AppState) -> Self {
        state.credential.clone()
    }
}

/// Build the Subsonic router: every endpoint under `/rest/`, behind the
/// salted-token auth middleware. Each endpoint is reachable both as `/rest/<n>`
/// and `/rest/<n>.view` — Subsonic clients append `.view` to the method name.
/// [`serve`] mounts this on a listener; it is also the seam integration tests
/// drive directly.
pub fn router(manager: LibraryManager, credential: SubsonicCredential) -> Router {
    let state = AppState {
        manager,
        credential: Arc::new(credential),
    };

    Router::new()
        .nest("/rest", endpoints::mount())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state)
}

/// Bind `addr` and serve the Subsonic API until `cancellation` fires. Mirrors
/// `bae-mcp`'s launch shape: a `TcpListener` bind, `axum::serve`, and graceful
/// shutdown driven by the cancellation token.
pub async fn serve(
    addr: std::net::SocketAddr,
    manager: LibraryManager,
    credential: SubsonicCredential,
    cancellation: CancellationToken,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let router = router(manager, credential);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancellation.cancelled_owned().await;
        })
        .await
        .inspect_err(|e| warn!("subsonic server stopped with error: {e}"))
}
