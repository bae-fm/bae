//! A Subsonic/OpenSubsonic REST server over a bae library.
//!
//! Exposes bae's library for browse and play to third-party Subsonic clients
//! (Symfonium, Amperfy, play:Sub, Feishin, …), reaching platforms bae ships no
//! native client to. The scope is the browse+play core; everything else
//! (playlists, star/rating, jukebox, podcasts, downloads, …) is out of scope.
//!
//! The desktop apps drive the server through [`SubsonicServerController`],
//! which runs its lifecycle on the shared `bae_util::server::ServerController`
//! (as `bae-mcp` does) — it binds the listener and runs `axum::serve` on the
//! [`router`] itself, so it can report a bind failure before it reports the
//! server running. [`router`] stays public as the seam the integration tests
//! drive. Nothing in `bae-library` depends on this crate.
//!
//! Implemented from the Subsonic API doc (subsonic.org) and the OpenSubsonic
//! spec (opensubsonic.netlify.app) only.

use axum::extract::FromRef;
use axum::Router;
use bae_app::AppServices;
use bae_diagnostics::Diagnostics;
use bae_preferences::SubsonicCredential;
use tokio::sync::watch;

mod auth;
mod controller;
mod endpoints;
mod envelope;
mod error;
mod id;
mod library_map;
mod model;
mod network;
mod params;
mod status;

pub use auth::token;
pub use controller::SubsonicServerController;
pub use id::{album_cover_art_id, song_id};
pub use network::{machine_addresses, AddressChanges, NetworkAddress, NetworkLister};
pub use status::{
    NetworkChoice, NotRunning, ServedAddress, SubsonicStatus, SubsonicView, SubsonicWatch,
};

/// Shared handler state: the library, the one accepted credential, and where a
/// panic in a request or a stream is reported.
#[derive(Clone)]
pub(crate) struct AppState {
    services: AppServices,
    credential: watch::Receiver<SubsonicCredential>,
    diagnostics: Diagnostics,
}

impl FromRef<AppState> for watch::Receiver<SubsonicCredential> {
    fn from_ref(state: &AppState) -> Self {
        state.credential.clone()
    }
}

/// Build the Subsonic router: every endpoint under `/rest/`, behind the
/// salted-token auth middleware. Each endpoint is reachable both as `/rest/<n>`
/// and `/rest/<n>.view` — Subsonic clients append `.view` to the method name.
/// It accepts the one `credential`, which is also the seam integration tests
/// drive it with. A request whose handling panics is reported to `diagnostics`
/// and answered with a Subsonic error document.
pub fn router(
    services: AppServices,
    credential: SubsonicCredential,
    diagnostics: Diagnostics,
) -> Router {
    router_following(services, watch::Sender::new(credential).subscribe(), diagnostics)
}

/// The router, accepting whatever credential `credential` holds as each
/// request arrives. [`SubsonicServerController`] mounts this on a listener, so
/// a changed username or password applies to the next sign-in.
pub fn router_following(
    services: AppServices,
    credential: watch::Receiver<SubsonicCredential>,
    diagnostics: Diagnostics,
) -> Router {
    let state = AppState { services, credential, diagnostics };

    Router::new()
        .nest("/rest", endpoints::mount())
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::require_auth))
        .layer(axum::middleware::from_fn_with_state(state.clone(), endpoints::answer_panics))
        .with_state(state)
}
