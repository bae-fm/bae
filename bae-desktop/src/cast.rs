//! Desktop-side Cast ownership: device discovery, the ephemeral Subsonic server
//! the receiver fetches audio from, and the URL providers that mint its media
//! links.
//!
//! The cast *session* lives in bae-core's playback service (cast is a renderer
//! behind the one queue). This module owns what the service can't: discovering
//! devices, serving audio over HTTP with a per-session credential, and turning a
//! device id into a connected channel plus the injected URL providers. It
//! mirrors how bae-desktop owns the mcp/subsonic controllers.

use std::sync::{Arc, Mutex};

use bae_core::cast::{
    CastDevice, CastDiscovery, CastStreamFormat, CoverUrlProvider, MediaUrlProvider,
    RustCastChannel, CAST_TRANSCODE_BITRATE_KBPS,
};
use bae_core::config::SubsonicCredential;
use bae_core::library::{AppServices, LibraryManager};
use bae_core::playback::{PlaybackHandle, PlaybackProgress};
use bae_core::ui::{Invalidation, UiBusEvent, UiEventBus};
use md5::{Digest, Md5};
use rand::RngCore;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// The current cast status, snapshot for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastStatus {
    NotCasting,
    Casting { device_name: String },
}

/// A failure starting a cast session.
#[derive(Debug)]
pub enum CastError {
    /// No discovered device matches the requested id.
    DeviceNotFound,
    /// The control channel to the device couldn't be opened.
    Connect(String),
    /// The ephemeral serving couldn't be started (bind / no LAN address).
    Serving(String),
}

impl std::fmt::Display for CastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CastError::DeviceNotFound => write!(f, "no such Cast device"),
            CastError::Connect(detail) => {
                write!(f, "couldn't connect to the Cast device: {detail}")
            }
            CastError::Serving(detail) => {
                write!(
                    f,
                    "couldn't start serving audio to the Cast device: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for CastError {}

/// The ephemeral Subsonic server the receiver fetches audio from: the
/// bae-subsonic router bound on a random LAN port with a per-session credential,
/// independent of the user's Subsonic settings. Alive only while a cast session
/// is.
struct EphemeralServer {
    /// `http://<lan-ip>:<port>` — the base the receiver fetches from.
    base_url: String,
    credential: SubsonicCredential,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl EphemeralServer {
    /// Bind the router on `0.0.0.0:0` (an OS-assigned port), read the port back,
    /// and serve until cancelled. The base URL names the machine's LAN address so
    /// the receiver can reach it.
    async fn start(manager: LibraryManager) -> Result<Self, String> {
        let credential = SubsonicCredential {
            username: random_hex(8),
            password: random_hex(32),
        };
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", 0))
            .await
            .map_err(|e| format!("bind failed: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("reading the bound port failed: {e}"))?
            .port();
        let lan_ip = local_ip_address::local_ip()
            .map_err(|e| format!("no LAN IP address available: {e}"))?;
        let base_url = format!("http://{lan_ip}:{port}");

        let router = bae_subsonic::router(manager, credential.clone());
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            let served = axum::serve(listener, router)
                .with_graceful_shutdown(async move { server_cancel.cancelled_owned().await })
                .await;
            if let Err(e) = served {
                warn!("ephemeral cast server stopped with error: {e}");
            }
        });

        Ok(Self {
            base_url,
            credential,
            cancel,
            task,
        })
    }

    async fn stop(self) {
        self.cancel.cancel();
        if let Err(e) = self.task.await {
            warn!("ephemeral cast server task join failed: {e}");
        }
    }
}

/// Owns everything the desktop side of casting needs: device discovery, the
/// ephemeral server (lazily started on the first cast, stopped when casting
/// ends), and the current status. A background task follows the playback
/// service's cast-status events to keep the status and the server lifecycle in
/// step — including a receiver-side end the service detects on its own.
pub struct CastController {
    runtime: Handle,
    manager: LibraryManager,
    playback: PlaybackHandle,
    discovery: Mutex<CastDiscovery>,
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    server: Option<EphemeralServer>,
    status: CastStatus,
}

impl CastController {
    pub fn new(services: &AppServices, ui_event_bus: UiEventBus, runtime: Handle) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            server: None,
            status: CastStatus::NotCasting,
        }));
        let discovery = CastDiscovery::new();
        // Forward device-list changes to the UI as an invalidation, so an open
        // picker requeries the list as devices come and go.
        Self::spawn_device_list_forwarder(discovery.subscribe(), ui_event_bus, &runtime);
        let controller = Self {
            runtime: runtime.clone(),
            manager: services.library_manager().clone(),
            playback: services.playback().clone(),
            discovery: Mutex::new(discovery),
            inner: inner.clone(),
        };
        controller.spawn_status_follower();
        controller
    }

    fn spawn_device_list_forwarder(
        mut devices: tokio::sync::watch::Receiver<Vec<CastDevice>>,
        ui_event_bus: UiEventBus,
        runtime: &Handle,
    ) {
        runtime.spawn(async move {
            while devices.changed().await.is_ok() {
                ui_event_bus.emit(UiBusEvent::Invalidated(Invalidation::CastDevices));
            }
        });
    }

    /// Follow the playback service's `CastStatusChanged` events: update the
    /// status, and stop the ephemeral server once casting ends (a user stop or a
    /// receiver-side end the service surfaced on its own).
    fn spawn_status_follower(&self) {
        let mut progress = self.playback.subscribe_progress();
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            while let Some(event) = progress.recv().await {
                let PlaybackProgress::CastStatusChanged { device_name } = event else {
                    continue;
                };
                match device_name {
                    Some(name) => {
                        inner.lock().unwrap().status = CastStatus::Casting { device_name: name };
                    }
                    None => {
                        // Take the server out under the lock, then stop it off the
                        // lock (its shutdown awaits).
                        let server = {
                            let mut guard = inner.lock().unwrap();
                            guard.status = CastStatus::NotCasting;
                            guard.server.take()
                        };
                        if let Some(server) = server {
                            server.stop().await;
                        }
                    }
                }
            }
        });
    }

    /// Start browsing for devices (the picker opened). Idempotent.
    pub fn start_discovery(&self) {
        self.discovery.lock().unwrap().start();
    }

    /// Stop browsing for devices (the picker closed).
    pub fn stop_discovery(&self) {
        self.discovery.lock().unwrap().stop();
    }

    /// The current device list plus a receiver that updates as devices come and
    /// go.
    pub fn devices(
        &self,
    ) -> (
        Vec<CastDevice>,
        tokio::sync::watch::Receiver<Vec<CastDevice>>,
    ) {
        let discovery = self.discovery.lock().unwrap();
        (discovery.devices(), discovery.subscribe())
    }

    pub fn status(&self) -> CastStatus {
        self.inner.lock().unwrap().status.clone()
    }

    /// Cast to the device named by `device_id`: connect its control channel,
    /// ensure the ephemeral server is serving, and hand the playback service the
    /// channel plus the URL providers.
    pub fn cast_to(&self, device_id: &str) -> Result<(), CastError> {
        let device = self
            .discovery
            .lock()
            .unwrap()
            .devices()
            .into_iter()
            .find(|device| device.id == device_id)
            .ok_or(CastError::DeviceNotFound)?;

        // The rust_cast connect blocks on the network; run it off the async
        // runtime so it never stalls a runtime thread.
        let (addr, port) = (device.addr, device.port);
        let channel = self
            .runtime
            .block_on(async move {
                tokio::task::spawn_blocking(move || RustCastChannel::connect(addr, port)).await
            })
            .map_err(|e| CastError::Connect(format!("connect task failed: {e}")))?
            .map_err(|e| CastError::Connect(e.to_string()))?;

        let (base_url, credential) = self.ensure_server()?;
        let stream_provider = stream_url_provider(base_url.clone(), credential.clone());
        let cover_provider = cover_url_provider(base_url, credential);

        // Optimistically reflect the target device; the service's event confirms
        // it (and drives the receiver-side-end transition back).
        self.inner.lock().unwrap().status = CastStatus::Casting {
            device_name: device.name.clone(),
        };
        self.playback.cast_to(
            Box::new(channel),
            device.name,
            stream_provider,
            cover_provider,
        );
        Ok(())
    }

    /// Stop casting: tell the playback service, which ends the session and
    /// announces the return to local; the status follower then stops the server.
    pub fn stop_casting(&self) {
        self.playback.stop_casting();
    }

    /// The ephemeral server's base URL and credential, starting it if it isn't
    /// already serving.
    fn ensure_server(&self) -> Result<(String, SubsonicCredential), CastError> {
        if let Some(info) = self
            .inner
            .lock()
            .unwrap()
            .server
            .as_ref()
            .map(|server| (server.base_url.clone(), server.credential.clone()))
        {
            return Ok(info);
        }
        let server = self
            .runtime
            .block_on(EphemeralServer::start(self.manager.clone()))
            .map_err(CastError::Serving)?;
        let info = (server.base_url.clone(), server.credential.clone());
        self.inner.lock().unwrap().server = Some(server);
        Ok(info)
    }
}

/// Build the stream-URL provider over a fixed base and credential. The format is
/// decided by the playback service (it knows the track's codec) and passed in;
/// this only renders the URL.
fn stream_url_provider(base_url: String, credential: SubsonicCredential) -> MediaUrlProvider {
    Arc::new(move |track_id: &str, format: CastStreamFormat| {
        let mut url = format!(
            "{base_url}/rest/stream?id=tr-{track_id}&{auth}&v=1.16.1&c=bae-cast",
            auth = auth_params(&credential),
        );
        match format {
            CastStreamFormat::Raw => url.push_str("&format=raw"),
            CastStreamFormat::TranscodeMp3 => {
                url.push_str(&format!(
                    "&format=mp3&maxBitRate={CAST_TRANSCODE_BITRATE_KBPS}"
                ));
            }
        }
        Ok(url)
    })
}

/// Build the cover-art-URL provider. Always yields a URL; a track with no cover
/// simply 404s on the receiver (it shows no art and plays on).
fn cover_url_provider(base_url: String, credential: SubsonicCredential) -> CoverUrlProvider {
    Arc::new(move |track_id: &str| {
        Some(format!(
            "{base_url}/rest/getCoverArt?id=tr-{track_id}&{auth}&v=1.16.1&c=bae-cast",
            auth = auth_params(&credential),
        ))
    })
}

/// The Subsonic salted-token auth query parameters for one request: `u`
/// (username), `t = md5(password + salt)`, and a fresh `s` (salt). The salt is
/// random per call, so no two URLs share one.
fn auth_params(credential: &SubsonicCredential) -> String {
    let salt = random_hex(8);
    let token = md5_hex(&format!("{}{}", credential.password, salt));
    format!("u={}&t={token}&s={salt}", credential.username)
}

/// Lowercase-hex md5 of `input` — the token derivation bae-subsonic's auth
/// recomputes and compares.
fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// `n` random bytes as lowercase hex (a 2n-char string).
fn random_hex(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> SubsonicCredential {
        SubsonicCredential {
            username: "castuser".to_string(),
            password: "castpassword".to_string(),
        }
    }

    /// A raw serve carries `format=raw`; a transcode carries
    /// `format=mp3&maxBitRate=320`. Both name the track as a `tr-` Subsonic id
    /// and carry the auth triplet.
    #[test]
    fn stream_url_encodes_format_and_track() {
        let provider = stream_url_provider("http://10.0.0.5:9000".to_string(), credential());

        let raw = provider("track-1", CastStreamFormat::Raw).unwrap();
        assert!(raw.starts_with("http://10.0.0.5:9000/rest/stream?id=tr-track-1&"));
        assert!(raw.contains("&format=raw"), "{raw}");
        assert!(raw.contains("u=castuser"));
        assert!(raw.contains("&t="));
        assert!(raw.contains("&s="));

        let transcoded = provider("track-2", CastStreamFormat::TranscodeMp3).unwrap();
        assert!(
            transcoded.contains("&format=mp3&maxBitRate=320"),
            "{transcoded}"
        );
    }

    /// The token in a minted URL is `md5(password + salt)` over the URL's own
    /// salt — the exact derivation bae-subsonic recomputes to authenticate the
    /// request.
    #[test]
    fn minted_token_matches_the_credential() {
        let credential = credential();
        let url = stream_url_provider("http://host:1".to_string(), credential.clone())(
            "t",
            CastStreamFormat::Raw,
        )
        .unwrap();

        let params: std::collections::HashMap<_, _> = url
            .split_once('?')
            .unwrap()
            .1
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .collect();
        let salt = params["s"];
        let expected = md5_hex(&format!("{}{salt}", credential.password));
        assert_eq!(params["t"], expected, "token must be md5(password + salt)");
    }

    /// Each minted URL carries a fresh salt, so the token differs between calls.
    #[test]
    fn each_url_uses_a_fresh_salt() {
        let provider = stream_url_provider("http://host:1".to_string(), credential());
        let a = provider("t", CastStreamFormat::Raw).unwrap();
        let b = provider("t", CastStreamFormat::Raw).unwrap();
        assert_ne!(a, b, "a fresh salt per call yields distinct URLs");
    }

    /// The ephemeral server binds a real port and authenticates the minted URLs
    /// through bae-subsonic's own auth: a request with the minted token reaches
    /// the handler (a missing track answers "not found", code 70 — not the
    /// wrong-credentials code 40), a tampered token is rejected (code 40), and
    /// stopping the server frees the port.
    #[test]
    fn ephemeral_server_authenticates_minted_urls_and_stops() {
        use bae_test_support as support;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (manager, _tmp) = support::setup_fresh_library(&runtime);

        runtime.block_on(async move {
            let server = EphemeralServer::start(manager)
                .await
                .expect("the ephemeral server binds a real port");

            let base = server.base_url.clone();
            let valid = stream_url_provider(base.clone(), server.credential.clone())(
                "missing-track",
                CastStreamFormat::Raw,
            )
            .unwrap();

            let body = reqwest::get(&valid)
                .await
                .expect("the server is reachable on its bound port")
                .text()
                .await
                .unwrap();
            assert!(
                !body.contains("code=\"40\""),
                "the minted token must authenticate (not a wrong-credentials error): {body}"
            );
            assert!(
                body.contains("code=\"70\""),
                "an authenticated request for a missing track answers not-found: {body}"
            );

            // Tamper with the token: auth must reject it.
            let tampered = valid.replace(
                &format!("&t={}", token_of(&valid)),
                "&t=00000000000000000000000000000000",
            );
            let tampered_body = reqwest::get(&tampered).await.unwrap().text().await.unwrap();
            assert!(
                tampered_body.contains("code=\"40\""),
                "a tampered token must be rejected with wrong-credentials: {tampered_body}"
            );

            server.stop().await;

            // The port is released: a fresh request fails to connect.
            assert!(
                reqwest::get(&valid).await.is_err(),
                "stopping the session must release the port"
            );
        });
    }

    /// Extract the `t` (token) value from a minted URL, for the tamper test.
    fn token_of(url: &str) -> String {
        url.split('&')
            .find_map(|pair| pair.strip_prefix("t="))
            .expect("the URL carries a token")
            .to_string()
    }
}
