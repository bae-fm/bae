//! Casting: the device protocols, device discovery, the ephemeral Subsonic
//! server the receiver fetches audio from, and the URL providers that mint its
//! media links.
//!
//! The cast *session* lives in bae-library's playback service (cast is a renderer
//! behind the one queue), which drives a device only through the interfaces it
//! defines: a Cast or UPnP device as a `RendererChannel`, an AirPlay receiver as
//! an `AirPlaySink`. This crate owns everything behind them: the Google Cast
//! (`cast`), UPnP (`dlna`) and AirPlay (`airplay`) wires, discovering
//! devices ([`renderer`]), serving audio over HTTP with a per-session
//! credential, and turning a device id into a connected channel or sink that it
//! hands the service.
//!
//! It is its own crate because it sits between two that must not depend on each
//! other: bae-library, which holds the renderer session, and bae-subsonic, which
//! serves the audio. Every host that casts — desktop and both mobile platforms —
//! builds one [`CastController`].

mod airplay;
mod cast;
mod dlna;
pub mod renderer;
#[cfg(test)]
mod test_support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bae_app::AppServices;
use bae_diagnostics::{lock_or_crash, Diagnostics, ReportedTask};
use bae_playback::airplay_output::AirPlaySink;
use bae_playback::renderer::{
    CoverUrlProvider, MediaUrlProvider, RendererChannel, RendererFlavor, RendererMediaSource,
    RendererStreamFormat, TRANSCODE_BITRATE_KBPS,
};
use bae_playback::{PlaybackProgress, RemoteDevice, RemotePlayback};
use bae_preferences::SubsonicCredential;
use bae_util::task_failure::{blocking, caught, caught_sync, TaskFailed};
use rand::RngCore;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use airplay::airplay2::TimingProtocol;
use airplay::capabilities::{Dialect, RaopEncryption};
use airplay::sink::{Ap2Sink, RaopSink};
use airplay::{AirPlayCapabilities, UnreadableSetting};
use cast::RustCastChannel;
use dlna::DlnaChannel;
use renderer::{
    RendererConnection, RendererDevice, RendererDiscovery, RendererServiceType, ReportedRenderer,
};

/// Build the push-audio sink for an AirPlay receiver, or a kind-specific error
/// when it can't be driven: one advertising a setting bae can't read, a
/// PIN-gated receiver, an AirPlay 2 receiver that doesn't take transient
/// pairing (the only AirPlay 2 pairing the sender does), or one offering only
/// encryption the sender can't provide. RAOP receivers pick unencrypted when
/// offered, else RSA-AES.
fn build_airplay_sink(
    diagnostics: &Diagnostics,
    addr: std::net::IpAddr,
    port: u16,
    capabilities: &Result<AirPlayCapabilities, UnreadableSetting>,
) -> Result<Box<dyn AirPlaySink>, CastError> {
    let capabilities = capabilities
        .as_ref()
        .map_err(|unreadable| CastError::AirPlayUnreadableSettings(unreadable.clone()))?;
    if capabilities.requires_pin {
        return Err(CastError::AirPlayPinRequired);
    }
    match &capabilities.dialect {
        Dialect::AirPlay2 if !capabilities.supports_transient_pairing => {
            Err(CastError::AirPlayPairingUnsupported)
        }
        Dialect::AirPlay2 => Ok(Box::new(Ap2Sink::new(
            addr,
            port,
            // The receiver's features decide whether it requires PTP or accepts NTP.
            TimingProtocol::from_features(capabilities.features),
            diagnostics.clone(),
        ))),
        Dialect::Raop { encryption: offered } => {
            let encryption = if offered.contains(&RaopEncryption::None) {
                RaopEncryption::None
            } else if offered.contains(&RaopEncryption::RsaAes) {
                RaopEncryption::RsaAes
            } else {
                return Err(CastError::AirPlayEncryptionUnsupported);
            };
            Ok(Box::new(RaopSink::new(
                addr,
                port,
                encryption,
                // The receiver plays at its hardware volume; bae attenuates locally
                // in the output drain, so seed the session at full.
                bae_playback::Volume::FULL,
                diagnostics.clone(),
            )))
        }
    }
}

/// A failure starting a cast session.
#[derive(Debug, thiserror::Error)]
pub enum CastError {
    /// Casting is turned off in settings, so no session may be started.
    #[error("casting is turned off in settings")]
    Disabled,
    /// No discovered device matches the requested id.
    #[error("no such Cast device")]
    DeviceNotFound,
    /// The control channel to the device couldn't be opened.
    #[error("couldn't connect to the Cast device: {0}")]
    Connect(String),
    /// The ephemeral serving couldn't be started (bind / no LAN address).
    #[error("couldn't start serving audio to the Cast device: {0}")]
    Serving(String),
    /// The AirPlay receiver demands a user PIN, which the sender doesn't support.
    #[error("this AirPlay receiver needs a PIN, which isn't supported")]
    AirPlayPinRequired,
    /// The AirPlay 2 receiver doesn't take transient pairing, the only AirPlay
    /// 2 pairing the sender does.
    #[error("this AirPlay receiver needs a pairing bae doesn't do")]
    AirPlayPairingUnsupported,
    /// The RAOP receiver only offers audio encryption the sender can't provide.
    #[error("this AirPlay receiver requires an encryption the sender can't provide")]
    AirPlayEncryptionUnsupported,
    /// The AirPlay receiver advertises a setting bae can't read, so how to
    /// connect to it can't be decided.
    #[error("this AirPlay receiver advertises settings bae can't read: {0}")]
    AirPlayUnreadableSettings(UnreadableSetting),
    /// Starting the session failed in bae itself: work it ran panicked, a bug
    /// reported where it was caught.
    #[error("{0}")]
    Internal(TaskFailed),
    /// Whether casting is on couldn't be read.
    #[error("couldn't read whether casting is on: {0}")]
    Setting(bae_db::DbError),
}

/// Where the ephemeral server listens, and the address its URLs name.
#[derive(Debug, Clone, Copy)]
enum ServingNetwork {
    /// Every interface, named by the machine's LAN address: a receiver on the
    /// local network must reach it.
    Lan,
    /// This machine only, named `127.0.0.1`: a test's requests stay off the
    /// network.
    #[cfg(test)]
    Loopback,
}

impl ServingNetwork {
    /// The address to bind and the address the URLs name.
    fn addresses(self) -> Result<(std::net::IpAddr, std::net::IpAddr), String> {
        match self {
            Self::Lan => {
                let lan_ip = local_ip_address::local_ip()
                    .map_err(|e| format!("no LAN IP address available: {e}"))?;
                Ok((std::net::Ipv4Addr::UNSPECIFIED.into(), lan_ip))
            }
            #[cfg(test)]
            Self::Loopback => {
                let loopback = std::net::Ipv4Addr::LOCALHOST.into();
                Ok((loopback, loopback))
            }
        }
    }
}

/// The ephemeral Subsonic server the receiver fetches audio from: the
/// bae-subsonic router bound on a random port with a per-session credential,
/// independent of the user's Subsonic settings. Alive only while a cast session
/// is.
struct EphemeralServer {
    /// `http://<ip>:<port>` — the base the receiver fetches from.
    base_url: String,
    credential: SubsonicCredential,
    cancel: CancellationToken,
    task: ReportedTask,
    /// Set once the server panicked: it serves nothing, and the next cast
    /// starts another.
    panicked: Arc<AtomicBool>,
}

impl EphemeralServer {
    /// Bind the router on `network` at an OS-assigned port, read the port back,
    /// and serve until cancelled. The base URL names the address `network`
    /// gives, so the receiver can reach it. A request that panics is reported
    /// through `diagnostics` and answered with an error document. A server
    /// that panics is reported there too, reads as [`Self::panicked`], and
    /// `on_panic` runs, for its owner to end the cast it served.
    async fn start(
        services: AppServices,
        network: ServingNetwork,
        diagnostics: &Diagnostics,
        on_panic: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let credential = SubsonicCredential { username: random_hex(8), password: random_hex(32) };
        let (bind_ip, url_ip) = network.addresses()?;
        let listener = tokio::net::TcpListener::bind((bind_ip, 0))
            .await
            .map_err(|e| format!("bind failed: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("reading the bound port failed: {e}"))?
            .port();
        let base_url = format!("http://{url_ip}:{port}");

        let router = bae_subsonic::router(services, credential.clone(), diagnostics.clone());
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let panicked = Arc::new(AtomicBool::new(false));
        let mark_panicked = panicked.clone();
        let task = diagnostics.spawn_reported(
            &Handle::current(),
            "serve audio to a cast receiver",
            async move {
                let served = axum::serve(listener, router)
                    .with_graceful_shutdown(async move { server_cancel.cancelled_owned().await })
                    .await;
                if let Err(e) = served {
                    warn!("ephemeral cast server stopped with error: {e}");
                }
            },
            move |_| {
                mark_panicked.store(true, Ordering::Release);
                on_panic();
            },
        );

        Ok(Self { base_url, credential, cancel, task, panicked })
    }

    /// Whether the server panicked and serves nothing.
    fn panicked(&self) -> bool {
        self.panicked.load(Ordering::Acquire)
    }

    async fn stop(self) {
        self.cancel.cancel();
        self.task.ended().await;
    }
}

/// Owns everything casting needs: device discovery and the ephemeral server
/// (lazily started on the first cast, stopped when casting ends). A background
/// task follows the playback service's remote-output status to keep the server
/// lifecycle in step with it — including a receiver-side end the service detects
/// on its own.
///
/// The whole of it is gated on the library's `cast_enabled` setting, which the
/// controller reads from the preferences row at each entry point rather than
/// mirroring: while casting is off nothing browses the network and no session
/// can start. A watcher follows the row and applies a change to the running
/// machinery.
pub struct CastController {
    runtime: Handle,
    services: AppServices,
    /// Where the picker's devices come from: bae's own browsing, or the services
    /// a host's browser reports in. Chosen once, per host. The picker reads
    /// them from the library's `cast_device` table, which the device list
    /// writer keeps.
    discovery: Mutex<RendererDiscovery>,
    /// Whether the picker asked for browsing and has not stopped it since.
    /// Set before the setting is read, and checked again under the discovery
    /// lock, so a stop that comes while the read is out is never undone.
    browsing_asked: AtomicBool,
    /// Whether browsing runs: asked for while casting was on, and neither
    /// stopped nor turned off since. Changed under the discovery lock; a
    /// renderer a host reports is taken only while it holds.
    browsing: AtomicBool,
    /// The ephemeral server, once a cast has started one.
    server: Arc<Mutex<Option<EphemeralServer>>>,
    /// Where that server listens.
    serving: ServingNetwork,
    /// Where casting's background work — the server, the watchers, the
    /// discovery readers and the AirPlay stream threads — reports a panic.
    diagnostics: Diagnostics,
    /// The `cast_enabled` value the setting watcher last finished applying, so
    /// a test can wait for the watcher to act on a change it wrote.
    #[cfg(test)]
    setting_applied: tokio::sync::watch::Sender<Option<bool>>,
    /// How many asks to browse have been decided, so a test can wait for
    /// the one it made.
    #[cfg(test)]
    browsing_decided: tokio::sync::watch::Sender<u64>,
}

impl CastController {
    /// Build the controller over a host's device source and start the background
    /// tasks it needs: the device-list writer that lists the devices for the picker,
    /// the follower that tracks the playback service's session status, and the
    /// watcher that applies the `cast_enabled` setting.
    pub fn start(
        services: AppServices,
        runtime: Handle,
        discovery: RendererDiscovery,
        diagnostics: Diagnostics,
    ) -> Arc<Self> {
        Self::start_serving_on(services, runtime, discovery, ServingNetwork::Lan, diagnostics)
    }

    /// [`Self::start`], with the ephemeral server on `serving`.
    fn start_serving_on(
        services: AppServices,
        runtime: Handle,
        discovery: RendererDiscovery,
        serving: ServingNetwork,
        diagnostics: Diagnostics,
    ) -> Arc<Self> {
        let receivers = discovery.subscribe();
        let controller = Arc::new(Self {
            runtime: runtime.clone(),
            services,
            discovery: Mutex::new(discovery),
            browsing_asked: AtomicBool::new(false),
            browsing: AtomicBool::new(false),
            server: Arc::new(Mutex::new(None)),
            serving,
            diagnostics,
            #[cfg(test)]
            setting_applied: tokio::sync::watch::Sender::new(None),
            #[cfg(test)]
            browsing_decided: tokio::sync::watch::Sender::new(0),
        });
        controller.spawn_device_list_writer(receivers);
        controller.spawn_status_follower();
        controller.spawn_setting_watcher();
        controller
    }

    /// List the devices for the picker: the merged list as it stands, then
    /// again after each change to any source's list, each a write of the
    /// whole list. One task writes them all, in order, so an older merge never
    /// lands after a newer one; changes that arrive while a write runs are
    /// merged into the next. A panic merging the lists is reported, and so is
    /// a write that fails; the next change is merged and written again. The
    /// writer ends once every source has.
    fn spawn_device_list_writer(self: &Arc<Self>, sources: Vec<crate::renderer::DeviceListWatch>) {
        let (changed, mut changes) = tokio::sync::mpsc::unbounded_channel();
        for mut source in sources {
            let changed = changed.clone();
            self.runtime.spawn(async move {
                while source.changed().await.is_ok() {
                    if changed.send(()).is_err() {
                        return;
                    }
                }
            });
        }
        drop(changed);
        let controller = self.clone();
        self.runtime.spawn(async move {
            loop {
                let merged = controller.with_discovery(|discovery| {
                    caught_sync("merge the cast device lists", || discovery.devices())
                });
                match merged {
                    Ok(merged) => {
                        let listed = merged.iter().map(RendererDevice::listed).collect();
                        if let Err(error) = controller.services.show_cast_devices(listed).await {
                            controller.diagnostics.report_bug(
                                "list the cast receivers",
                                &bae_util::error_text::CauseChain(&error),
                            );
                        }
                    }
                    Err(bug) => {
                        controller.diagnostics.panicked(bug);
                    }
                }
                if changes.recv().await.is_none() {
                    return;
                }
                while changes.try_recv().is_ok() {}
            }
        });
    }

    /// Follow the library's `cast_enabled` setting: turning casting off stops
    /// browsing and ends any session in flight, so the machinery is idle rather
    /// than merely hidden; turning it on starts nothing by itself, since browsing
    /// follows the picker. The first value is where the setting stood as the
    /// controller started, when nothing browsed or cast; only a turn from on
    /// to off after it is applied. A panic applying one change is reported,
    /// and the next change is applied.
    fn spawn_setting_watcher(self: &Arc<Self>) {
        let setting = self.services.follow_preferences(|prefs| prefs.cast_enabled);
        let controller = self.clone();
        self.runtime.spawn(async move {
            let mut was_enabled = None;
            loop {
                match setting.next().await {
                    Ok(enabled) => {
                        let turned_off = was_enabled == Some(true) && !enabled;
                        was_enabled = Some(enabled);
                        if turned_off {
                            let applied = caught_sync("turn casting off", || {
                                controller.stop_discovery();
                                controller.stop_casting();
                            });
                            if let Err(bug) = applied {
                                controller.diagnostics.panicked(bug);
                            }
                        }
                        #[cfg(test)]
                        controller.setting_applied.send_replace(Some(enabled));
                    }
                    Err(bae_db::live_query::LiveQueryError::Query(error)) => {
                        warn!("couldn't read whether casting is on: {error}");
                    }
                    Err(bae_db::live_query::LiveQueryError::Cancelled) => {
                        debug!("cast setting watcher stopped: the library closed");
                        break;
                    }
                }
            }
        });
    }

    /// Follow the playback service's remote-output status and stop the ephemeral
    /// server once casting ends (a user stop or a receiver-side end the service
    /// surfaced on its own). A panic stopping it is reported, and the next end
    /// is followed.
    fn spawn_status_follower(&self) {
        let mut progress = self.services.subscribe_playback_progress();
        let slot = self.server.clone();
        let diagnostics = self.diagnostics.clone();
        self.runtime.spawn(async move {
            while let Some(event) = progress.recv().await {
                let PlaybackProgress::RemoteStatusChanged {
                    remote: RemotePlayback::Local | RemotePlayback::Ended { .. },
                } = event
                else {
                    continue;
                };
                let stopped = caught("stop the cast server", async {
                    // Take the server out under the lock, then stop it off the
                    // lock (its shutdown awaits).
                    let server = slot.lock().unwrap().take();
                    if let Some(server) = server {
                        server.stop().await;
                    }
                })
                .await;
                if let Err(bug) = stopped {
                    diagnostics.panicked(bug);
                }
            }
        });
    }

    /// Act on the device sources. A panic while they were held may have left
    /// a browse partway through starting or stopping, holding a daemon,
    /// socket or thread nothing tracks: casting can't go on from that.
    fn with_discovery<T>(&self, act: impl FnOnce(&mut RendererDiscovery) -> T) -> T {
        act(&mut lock_or_crash(&self.diagnostics, "the cast device sources", &self.discovery))
    }

    /// Whether the library has casting turned on, read from the preferences
    /// row on demand: the row is the one authority, so there is no copy here
    /// to fall out of step with it.
    async fn enabled(&self) -> Result<bool, CastError> {
        Ok(self.services.preferences().await.map_err(CastError::Setting)?.cast_enabled)
    }

    /// Start browsing for devices (the picker opened). Idempotent, and a no-op
    /// while casting is off — this is the gate that keeps the network quiet, not
    /// the picker's visibility. The setting is read off the caller's thread;
    /// browsing starts once it is, unless the picker closed meanwhile.
    pub fn start_discovery(self: &Arc<Self>) {
        self.browsing_asked.store(true, Ordering::Release);
        let controller = self.clone();
        self.runtime.spawn(async move {
            match controller.enabled().await {
                Ok(true) => controller.with_discovery(|discovery| {
                    if controller.browsing_asked.load(Ordering::Acquire) {
                        discovery.start(&controller.diagnostics);
                        controller.browsing.store(true, Ordering::Release);
                    }
                }),
                Ok(false) => debug!("cast discovery not started: casting is off"),
                Err(error) => warn!("cast discovery not started: {error}"),
            }
            #[cfg(test)]
            controller.browsing_decided.send_modify(|decided| *decided += 1);
        });
    }

    /// Stop browsing (the picker closed, or casting was turned off).
    pub fn stop_discovery(&self) {
        self.browsing_asked.store(false, Ordering::Release);
        self.with_discovery(|discovery| {
            self.browsing.store(false, Ordering::Release);
            discovery.stop();
        });
    }

    /// Take a renderer service a host's browser resolved, on the hosts that
    /// browse for bae rather than letting it read the network itself. Refused
    /// while bae is not browsing, which it never is while casting is off.
    pub fn renderer_found(&self, service: ReportedRenderer) {
        // Mapping a service runs over what a host browser read off the
        // network; one that panics is reported and skipped, and the panic
        // stays inside the lock's hold, so it poisons nothing.
        let reported = self.with_discovery(|discovery| {
            if !self.browsing.load(Ordering::Acquire) {
                debug!("reported renderer ignored: bae is not browsing");
                return Ok(());
            }
            caught_sync("take a reported renderer", || discovery.report(service))
        });
        if let Err(bug) = reported {
            self.diagnostics.panicked(bug);
        }
    }

    /// Drop a renderer service a host's browser no longer sees.
    pub fn renderer_lost(&self, service_type: RendererServiceType, instance_name: &str) {
        self.with_discovery(|discovery| discovery.forget(service_type, instance_name));
    }

    /// The current merged device list — Cast, UPnP, and AirPlay devices in one
    /// list, sorted by name for a stable picker.
    pub fn devices(&self) -> Vec<RendererDevice> {
        self.with_discovery(|discovery| discovery.devices())
    }

    /// Play to the device named by `device_id`: build its control channel from
    /// the device's connection (Cast or UPnP), ensure the ephemeral server is
    /// serving, and hand the playback service the channel plus the media source
    /// the device fetches each track through.
    pub async fn cast_to(&self, device_id: &str) -> Result<(), CastError> {
        if !self.enabled().await? {
            return Err(CastError::Disabled);
        }
        let device = self
            .devices()
            .into_iter()
            .find(|device| device.id == device_id)
            .ok_or(CastError::DeviceNotFound)?;

        // AirPlay is not a fetch-a-URL renderer: build the push-audio sink and hand
        // it to playback, which keeps decoding locally. No ephemeral server needed.
        if let RendererConnection::AirPlay { addr, port, capabilities } = &device.connection {
            let sink = build_airplay_sink(&self.diagnostics, *addr, *port, capabilities)?;
            self.services
                .playback_play_on_airplay(sink, RemoteDevice { id: device.id, name: device.name });
            return Ok(());
        }

        self.start_fetch_renderer(device).await
    }

    /// Build the control channel for a device, and name its flavor, which
    /// decides what it is served. Cast connects over the network (run off the runtime so it
    /// never stalls a runtime thread); UPnP has no handshake — each SOAP action
    /// is its own request.
    async fn start_fetch_renderer(&self, device: RendererDevice) -> Result<(), CastError> {
        let (channel, flavor): (Box<dyn RendererChannel>, RendererFlavor) = match &device.connection
        {
            RendererConnection::Cast { addr, port } => {
                let (addr, port) = (*addr, *port);
                let channel = blocking("connect to the Cast device", move || {
                    RustCastChannel::connect(addr, port)
                })
                .await
                .map_err(|failed| CastError::Internal(self.diagnostics.bug(failed)))?
                .map_err(|e| CastError::Connect(e.to_string()))?;
                (Box::new(channel), RendererFlavor::Cast)
            }
            RendererConnection::Dlna { av_transport_url, rendering_control_url } => {
                let channel =
                    DlnaChannel::connect(av_transport_url.clone(), rendering_control_url.clone())
                        .map_err(|e| CastError::Connect(e.to_string()))?;
                (Box::new(channel), RendererFlavor::Dlna)
            }
            // AirPlay is handled in `cast_to` before it reaches here — it has no
            // fetch-a-URL channel.
            RendererConnection::AirPlay { .. } => {
                return Err(CastError::Connect(
                    "AirPlay does not use a control channel".to_string(),
                ));
            }
        };

        self.ensure_server().await?;
        let (stream_provider, cover_provider) = {
            let slot = self.server.lock().unwrap();
            let server = slot.as_ref().expect("ensure_server installed the ephemeral server");
            (
                stream_url_provider(server.base_url.clone(), server.credential.clone()),
                cover_url_provider(server.base_url.clone(), server.credential.clone()),
            )
        };

        self.services.playback_play_on(
            channel,
            RemoteDevice { id: device.id, name: device.name },
            RendererMediaSource::new(stream_provider, cover_provider, flavor),
        );
        Ok(())
    }

    /// Stop casting: tell the playback service, which ends the session and
    /// announces the return to local; the status follower then stops the server.
    pub fn stop_casting(&self) {
        self.services.playback_stop_remote();
    }

    async fn ensure_server(&self) -> Result<(), CastError> {
        let panicked = {
            let mut slot = self.server.lock().unwrap();
            match slot.as_ref() {
                Some(server) if !server.panicked() => return Ok(()),
                Some(_) => slot.take(),
                None => None,
            }
        };
        if let Some(panicked) = panicked {
            panicked.stop().await;
        }
        // A server that panics serves nothing more: end the cast it served,
        // and the next cast starts another in its place.
        let services = self.services.clone();
        let server = EphemeralServer::start(
            self.services.clone(),
            self.serving,
            &self.diagnostics,
            move || services.playback_stop_remote(),
        )
        .await
        .map_err(CastError::Serving)?;
        *self.server.lock().unwrap() = Some(server);
        Ok(())
    }
}

/// Build the stream-URL provider over a fixed base and credential. The format is
/// decided by the playback service (it knows the track's codec) and passed in;
/// this only renders the URL.
fn stream_url_provider(base_url: String, credential: SubsonicCredential) -> MediaUrlProvider {
    Arc::new(move |track_id: &str, format: RendererStreamFormat| {
        let mut url = format!(
            "{base_url}/rest/stream?id={song}&{auth}&v=1.16.1&c=bae-cast",
            song = bae_subsonic::song_id(track_id),
            auth = auth_params(&credential),
        );
        match format {
            RendererStreamFormat::Raw => url.push_str("&format=raw"),
            RendererStreamFormat::TranscodeMp3 => {
                url.push_str(&format!("&format=mp3&maxBitRate={TRANSCODE_BITRATE_KBPS}"));
            }
            RendererStreamFormat::TranscodeWav => url.push_str("&format=wav"),
        }
        Ok(url)
    })
}

/// Build the cover-art-URL provider. The cover is named by its `coverArt`
/// id, which carries its version, so a replaced cover has a new URL, which a
/// receiver that cached the old art fetches again.
fn cover_url_provider(base_url: String, credential: SubsonicCredential) -> CoverUrlProvider {
    Arc::new(move |cover: &bae_db::ImageRef| {
        format!(
            "{base_url}/rest/getCoverArt?id={cover_art}&{auth}&v=1.16.1&c=bae-cast",
            cover_art = bae_subsonic::album_cover_art_id(&cover.id, &cover.version),
            auth = auth_params(&credential),
        )
    })
}

/// The Subsonic salted-token auth query parameters for one request: `u`
/// (username), `t = md5(password + salt)`, and a fresh `s` (salt). The salt is
/// random per call, so no two URLs share one.
fn auth_params(credential: &SubsonicCredential) -> String {
    let salt = random_hex(8);
    let token = bae_subsonic::token(&credential.password, &salt);
    format!("u={}&t={token}&s={salt}", credential.username)
}

/// `n` random bytes as lowercase hex (a 2n-char string).
fn random_hex(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
