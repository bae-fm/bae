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

use bae_core::airplay::airplay2::TimingProtocol;
use bae_core::airplay::capabilities::{Dialect, RaopEncryption};
use bae_core::airplay::{AirPlayCapabilities, AirPlayDiscovery};
use bae_core::cast::{CastDiscovery, RustCastChannel};
use bae_core::config::SubsonicCredential;
use bae_core::dlna::{DlnaChannel, DlnaDiscovery};
use bae_core::library::LibraryManager;
use bae_core::playback::airplay_output::{AirPlaySink, Ap2Sink, RaopSink};
use bae_core::playback::{PlaybackHandle, PlaybackProgress};
use bae_core::renderer::{
    cast_stream_format, dlna_stream_format, CoverUrlProvider, MediaUrlProvider, RendererChannel,
    RendererConnection, RendererDevice, RendererStreamFormat, StreamFormatFn,
    TRANSCODE_BITRATE_KBPS,
};
use bae_core::ui::{Invalidation, UiBusEvent, UiEventBus};
use md5::{Digest, Md5};
use rand::RngCore;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// The RAOP audio latency the sender assumes when a receiver doesn't report one
/// — ~2 s at 44.1 kHz. Used both to pace the stream ahead and to offset the
/// position the UI shows back to what is audible.
const AIRPLAY_LATENCY_FRAMES: u32 = 88_200;

/// Build the push-audio sink for an AirPlay receiver, or a kind-specific error
/// when it can't be driven: a PIN-gated receiver, an AirPlay-2-only receiver
/// (whose streaming isn't wired yet), or one offering only encryption the sender
/// can't provide. RAOP receivers pick unencrypted when offered, else RSA-AES.
fn build_airplay_sink(
    addr: std::net::IpAddr,
    port: u16,
    capabilities: &AirPlayCapabilities,
) -> Result<Box<dyn AirPlaySink>, CastError> {
    if capabilities.requires_pin {
        return Err(CastError::AirPlayPinRequired);
    }
    match capabilities.dialect {
        Dialect::AirPlay2 => Ok(Box::new(Ap2Sink {
            receiver: addr,
            airplay_port: port,
            latency_frames: Some(AIRPLAY_LATENCY_FRAMES),
            // The receiver's features decide whether it requires PTP or accepts NTP.
            timing: TimingProtocol::from_features(capabilities.features),
        })),
        Dialect::Raop => {
            let raop = capabilities
                .raop
                .as_ref()
                .ok_or(CastError::AirPlayEncryptionUnsupported)?;
            let encryption = if raop.encryption.contains(&RaopEncryption::None) {
                RaopEncryption::None
            } else if raop.encryption.contains(&RaopEncryption::RsaAes) {
                RaopEncryption::RsaAes
            } else {
                return Err(CastError::AirPlayEncryptionUnsupported);
            };
            Ok(Box::new(RaopSink {
                receiver: addr,
                rtsp_port: port,
                encryption,
                latency_frames: Some(AIRPLAY_LATENCY_FRAMES),
                // The receiver plays at its hardware volume; bae attenuates locally
                // in the output drain, so seed the session at full.
                initial_volume: 1.0,
            }))
        }
    }
}

/// The current cast status, snapshot for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastStatus {
    NotCasting,
    Casting { device_name: String },
}

/// A failure starting a cast session.
#[derive(Debug)]
pub enum CastError {
    /// Casting is turned off in settings, so no session may be started.
    Disabled,
    /// No discovered device matches the requested id.
    DeviceNotFound,
    /// The control channel to the device couldn't be opened.
    Connect(String),
    /// The ephemeral serving couldn't be started (bind / no LAN address).
    Serving(String),
    /// The AirPlay receiver demands a user PIN, which the sender doesn't support.
    AirPlayPinRequired,
    /// The RAOP receiver only offers audio encryption the sender can't provide.
    AirPlayEncryptionUnsupported,
}

impl std::fmt::Display for CastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CastError::Disabled => write!(f, "casting is turned off in settings"),
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
            CastError::AirPlayPinRequired => {
                write!(
                    f,
                    "this AirPlay receiver needs a PIN, which isn't supported"
                )
            }
            CastError::AirPlayEncryptionUnsupported => write!(
                f,
                "this AirPlay receiver requires an encryption the sender can't provide"
            ),
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
///
/// The whole of it is gated on the library's `cast_enabled` setting, which the
/// controller reads from config at each entry point rather than mirroring: while
/// casting is off nothing browses the network and no session can start.
pub struct CastController {
    runtime: Handle,
    manager: LibraryManager,
    playback: PlaybackHandle,
    /// Google Cast discovery (mDNS) and UPnP discovery (SSDP) run side by side;
    /// their device lists are merged into one for the picker — a speaker is a
    /// speaker, whatever its protocol.
    cast_discovery: Mutex<CastDiscovery>,
    dlna_discovery: Mutex<DlnaDiscovery>,
    airplay_discovery: Mutex<AirPlayDiscovery>,
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    server: Option<EphemeralServer>,
    status: CastStatus,
}

impl CastController {
    pub fn new(
        manager: LibraryManager,
        playback: PlaybackHandle,
        ui_event_bus: UiEventBus,
        runtime: Handle,
    ) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            server: None,
            status: CastStatus::NotCasting,
        }));
        let cast_discovery = CastDiscovery::new();
        let dlna_discovery = DlnaDiscovery::new();
        let airplay_discovery = AirPlayDiscovery::new();
        // Forward each discovery's list changes to the UI as an invalidation, so
        // an open picker requeries the merged list as devices come and go.
        Self::spawn_device_list_forwarder(
            cast_discovery.subscribe(),
            ui_event_bus.clone(),
            &runtime,
        );
        Self::spawn_device_list_forwarder(
            dlna_discovery.subscribe(),
            ui_event_bus.clone(),
            &runtime,
        );
        Self::spawn_device_list_forwarder(airplay_discovery.subscribe(), ui_event_bus, &runtime);
        let controller = Self {
            runtime: runtime.clone(),
            manager,
            playback,
            cast_discovery: Mutex::new(cast_discovery),
            dlna_discovery: Mutex::new(dlna_discovery),
            airplay_discovery: Mutex::new(airplay_discovery),
            inner: inner.clone(),
        };
        controller.spawn_status_follower();
        controller
    }

    fn spawn_device_list_forwarder(
        mut devices: tokio::sync::watch::Receiver<Vec<RendererDevice>>,
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
                let PlaybackProgress::RemoteStatusChanged { device_name } = event else {
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

    /// Whether the library has casting turned on. Read from config on demand:
    /// config is the one authority, so there is no copy here to fall out of step
    /// with a change made on this device or synced from another.
    fn enabled(&self) -> bool {
        self.manager.get_config().cast_enabled
    }

    /// Start browsing for devices on every protocol (the picker opened).
    /// Idempotent, and a no-op while casting is off — this is the gate that
    /// keeps mDNS and SSDP sockets closed, not the picker's visibility.
    pub fn start_discovery(&self) {
        if !self.enabled() {
            debug!("cast discovery not started: casting is off");
            return;
        }
        self.cast_discovery.lock().unwrap().start();
        self.dlna_discovery.lock().unwrap().start();
        self.airplay_discovery.lock().unwrap().start();
    }

    /// Apply the library's `cast_enabled` setting. Turning casting off stops
    /// browsing and ends any session in flight, so the machinery is idle rather
    /// than merely hidden; turning it on starts nothing by itself, since
    /// browsing follows the picker.
    pub fn apply_enabled(&self, enabled: bool) {
        if enabled {
            return;
        }
        self.stop_discovery();
        self.stop_casting();
    }

    /// Stop browsing on every protocol (the picker closed).
    pub fn stop_discovery(&self) {
        self.cast_discovery.lock().unwrap().stop();
        self.dlna_discovery.lock().unwrap().stop();
        self.airplay_discovery.lock().unwrap().stop();
    }

    /// The current merged device list — Cast, UPnP, and AirPlay devices in one
    /// list, sorted by name for a stable picker.
    pub fn devices(&self) -> Vec<RendererDevice> {
        let mut devices = self.cast_discovery.lock().unwrap().devices();
        devices.extend(self.dlna_discovery.lock().unwrap().devices());
        devices.extend(self.airplay_discovery.lock().unwrap().devices());
        devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        devices
    }

    pub fn status(&self) -> CastStatus {
        self.inner.lock().unwrap().status.clone()
    }

    /// Play to the device named by `device_id`: build its control channel from
    /// the device's connection (Cast or UPnP), ensure the ephemeral server is
    /// serving, and hand the playback service the channel plus the URL providers
    /// and the flavor's stream-format gate.
    pub fn cast_to(&self, device_id: &str) -> Result<(), CastError> {
        if !self.enabled() {
            return Err(CastError::Disabled);
        }
        let device = self
            .devices()
            .into_iter()
            .find(|device| device.id == device_id)
            .ok_or(CastError::DeviceNotFound)?;

        // AirPlay is not a fetch-a-URL renderer: build the push-audio sink and hand
        // it to playback, which keeps decoding locally. No ephemeral server needed.
        if let RendererConnection::AirPlay {
            addr,
            port,
            capabilities,
        } = &device.connection
        {
            let sink = build_airplay_sink(*addr, *port, capabilities)?;
            self.inner.lock().unwrap().status = CastStatus::Casting {
                device_name: device.name.clone(),
            };
            self.playback
                .play_on_airplay(sink, device.name, AIRPLAY_LATENCY_FRAMES);
            return Ok(());
        }

        let (channel, stream_format) = self.build_channel(&device.connection)?;

        let (base_url, credential) = self.ensure_server()?;
        let stream_provider = stream_url_provider(base_url.clone(), credential.clone());
        let cover_provider = cover_url_provider(base_url, credential);

        // Optimistically reflect the target device; the service's event confirms
        // it (and drives the device-side-end transition back).
        self.inner.lock().unwrap().status = CastStatus::Casting {
            device_name: device.name.clone(),
        };
        self.playback.play_on(
            channel,
            device.name,
            stream_provider,
            cover_provider,
            stream_format,
        );
        Ok(())
    }

    /// Build the control channel for a device, and the stream-format gate its
    /// flavor uses. Cast connects over the network (run off the runtime so it
    /// never stalls a runtime thread); UPnP has no handshake — each SOAP action
    /// is its own request.
    fn build_channel(
        &self,
        connection: &RendererConnection,
    ) -> Result<(Box<dyn RendererChannel>, StreamFormatFn), CastError> {
        match connection {
            RendererConnection::Cast { addr, port } => {
                let (addr, port) = (*addr, *port);
                let channel = self
                    .runtime
                    .block_on(async move {
                        tokio::task::spawn_blocking(move || RustCastChannel::connect(addr, port))
                            .await
                    })
                    .map_err(|e| CastError::Connect(format!("connect task failed: {e}")))?
                    .map_err(|e| CastError::Connect(e.to_string()))?;
                Ok((Box::new(channel), cast_stream_format))
            }
            RendererConnection::Dlna {
                av_transport_url,
                rendering_control_url,
            } => {
                let channel =
                    DlnaChannel::connect(av_transport_url.clone(), rendering_control_url.clone())
                        .map_err(|e| CastError::Connect(e.to_string()))?;
                Ok((Box::new(channel), dlna_stream_format))
            }
            // AirPlay is handled in `cast_to` before it reaches here — it has no
            // fetch-a-URL channel.
            RendererConnection::AirPlay { .. } => Err(CastError::Connect(
                "AirPlay does not use a control channel".to_string(),
            )),
        }
    }

    /// Stop casting: tell the playback service, which ends the session and
    /// announces the return to local; the status follower then stops the server.
    pub fn stop_casting(&self) {
        self.playback.stop_remote();
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
    Arc::new(move |track_id: &str, format: RendererStreamFormat| {
        let mut url = format!(
            "{base_url}/rest/stream?id=tr-{track_id}&{auth}&v=1.16.1&c=bae-cast",
            auth = auth_params(&credential),
        );
        match format {
            RendererStreamFormat::Raw => url.push_str("&format=raw"),
            RendererStreamFormat::TranscodeMp3 => {
                url.push_str(&format!("&format=mp3&maxBitRate={TRANSCODE_BITRATE_KBPS}"));
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

    /// Whether any protocol is browsing right now — the three discoveries start
    /// and stop together, so any one of them running means sockets are open.
    fn browsing(controller: &CastController) -> bool {
        controller.cast_discovery.lock().unwrap().is_browsing()
            || controller.dlna_discovery.lock().unwrap().is_browsing()
            || controller.airplay_discovery.lock().unwrap().is_browsing()
    }

    fn test_controller(
        runtime: &tokio::runtime::Runtime,
    ) -> (CastController, LibraryManager, tempfile::TempDir) {
        use bae_test_support as support;

        let (manager, tmp) = support::setup_fresh_library(runtime);
        let playback = bae_core::playback::PlaybackService::start(
            manager.clone(),
            runtime.handle().clone(),
            1000,
            false,
        );
        let controller = CastController::new(
            manager.clone(),
            playback,
            UiEventBus::new(),
            runtime.handle().clone(),
        );
        (controller, manager, tmp)
    }

    /// The gate the setting exists for: with casting off, asking to browse opens
    /// no mDNS/SSDP socket and asking to cast is refused outright — the UI hiding
    /// its Cast control is a consequence, not the mechanism. Turning the setting
    /// on makes the same two calls do their work.
    #[test]
    fn discovery_and_casting_follow_the_cast_setting() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (controller, manager, _tmp) = test_controller(&runtime);

        assert!(!manager.get_config().cast_enabled, "casting is opt-in");
        controller.start_discovery();
        assert!(!browsing(&controller), "casting is off: nothing may browse");
        assert!(
            matches!(controller.cast_to("some-device"), Err(CastError::Disabled)),
            "casting is off: a session must be refused"
        );

        manager.set_cast_enabled(true).unwrap();
        controller.start_discovery();
        assert!(browsing(&controller), "casting is on: browsing may run");
        // The device is not on this test's network, so the request reaches the
        // real lookup and fails there rather than at the gate.
        assert!(matches!(
            controller.cast_to("some-device"),
            Err(CastError::DeviceNotFound)
        ));

        controller.stop_discovery();
    }

    /// Turning the setting off while browsing stops it, rather than leaving the
    /// sockets open until the picker happens to close.
    #[test]
    fn turning_casting_off_stops_browsing() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (controller, manager, _tmp) = test_controller(&runtime);

        manager.set_cast_enabled(true).unwrap();
        controller.start_discovery();
        assert!(browsing(&controller));

        manager.set_cast_enabled(false).unwrap();
        controller.apply_enabled(false);
        assert!(!browsing(&controller), "turning casting off stops browsing");
    }

    /// A raw serve carries `format=raw`; a transcode carries
    /// `format=mp3&maxBitRate=320`. Both name the track as a `tr-` Subsonic id
    /// and carry the auth triplet.
    #[test]
    fn stream_url_encodes_format_and_track() {
        let provider = stream_url_provider("http://10.0.0.5:9000".to_string(), credential());

        let raw = provider("track-1", RendererStreamFormat::Raw).unwrap();
        assert!(raw.starts_with("http://10.0.0.5:9000/rest/stream?id=tr-track-1&"));
        assert!(raw.contains("&format=raw"), "{raw}");
        assert!(raw.contains("u=castuser"));
        assert!(raw.contains("&t="));
        assert!(raw.contains("&s="));

        let transcoded = provider("track-2", RendererStreamFormat::TranscodeMp3).unwrap();
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
            RendererStreamFormat::Raw,
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
        let a = provider("t", RendererStreamFormat::Raw).unwrap();
        let b = provider("t", RendererStreamFormat::Raw).unwrap();
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
                RendererStreamFormat::Raw,
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
