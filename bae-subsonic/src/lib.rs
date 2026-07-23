//! A Subsonic/OpenSubsonic REST server over a bae library.
//!
//! Exposes bae's library for browse and play to third-party Subsonic clients
//! (Symfonium, Amperfy, play:Sub, Feishin, …), reaching platforms bae ships no
//! native client to. The scope is the browse+play core; everything else
//! (playlists, star/rating, jukebox, podcasts, downloads, …) is out of scope.
//!
//! The desktop apps drive the server through [`SubsonicServerController`], which
//! owns its lifecycle the way `bae-mcp`'s `McpServerController` does — it binds
//! the listener and runs `axum::serve` on the [`router`] itself, so it can
//! report a bind failure before it reports the server running. [`router`] stays
//! public as the seam the integration tests drive. Nothing in `bae-core`
//! depends on this crate.
//!
//! Implemented from the Subsonic API doc (subsonic.org) and the OpenSubsonic
//! spec (opensubsonic.netlify.app) only.
#![deny(unreachable_pub, dead_code)]

use std::sync::Arc;

use axum::extract::FromRef;
use axum::Router;
use bae_core::config::SubsonicCredential;
use bae_core::library::LibraryManager;

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
/// [`SubsonicServerController`] mounts this on a listener; it is also the seam
/// integration tests drive directly.
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
