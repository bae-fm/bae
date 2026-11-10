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

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bae_app::AppServices;
use bae_diagnostics::{lock_or_crash, Diagnostics, ReportedTask};
use bae_playback::airplay_output::AirPlaySink;
use bae_playback::renderer::{
    CoverUrlProvider, MediaUrlProvider, RendererChannel, RendererFlavor, RendererMediaSource,
    RendererStreamFormat, TRANSCODE_BITRATE_KBPS,
};
use bae_playback::{PlaybackProgress, RemotePlayback};
use bae_preferences::SubsonicCredential;
use bae_util::task_failure::{caught, caught_sync, TaskFailed};
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
    ids: &coven::IdSourceRef,
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
            ids.clone(),
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
    Setting(bae_store::DbError),
    /// Casting stopped, or was turned off, or another cast started, while
    /// this one connected. Playback stayed where it was.
    #[error("the cast was called off before it started")]
    CalledOff,
}

impl CastError {
    /// The class a surface words this failure in. An AirPlay receiver the
    /// sender can't drive, or whose settings can't be read, has its own line;
    /// a setting that couldn't be read is the library's database; anything
    /// else is a fault the person can only report.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::AirPlayPinRequired
            | Self::AirPlayPairingUnsupported
            | Self::AirPlayEncryptionUnsupported => C::AirPlayUnsupported,
            Self::AirPlayUnreadableSettings(_) => C::AirPlayUnreadableSettings,
            Self::Setting(error) => bae_diagnostics::database_category(error),
            Self::Disabled
            | Self::DeviceNotFound
            | Self::Connect(_)
            | Self::Serving(_)
            | Self::Internal(_)
            | Self::CalledOff => C::Internal,
        }
    }
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
    /// gives, so the receiver can reach it. A request that panics is answered
    /// with an error document. A server that panics reads as
    /// [`Self::panicked`], and `on_panic` runs, for its owner to end the cast
    /// it served.
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

/// A place that lists the receivers bae finds, and so asks it to browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowsingFor {
    /// The picker, while it is open.
    Picker,
    /// The Casting settings page, while it is open with casting on.
    SettingsPage,
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
    /// Casting's background tasks, started by [`Self::start_task`]. Dropping
    /// the controller aborts them; none holds the controller strongly, so
    /// nothing keeps it alive.
    tasks: Mutex<Vec<ReportedTask>>,
    services: AppServices,
    /// Where the picker's devices come from: bae's own browsing, or the services
    /// a host's browser reports in. Chosen once, per host. The picker reads
    /// them from the library's `cast_receiver` table, which the device list
    /// writer keeps.
    discovery: Mutex<RendererDiscovery>,
    /// The places open that list receivers. Named, not counted, so a second
    /// start or stop from one place changes nothing. Changed before the
    /// setting is read, and checked under the discovery lock, so a stop that
    /// comes while the read is out is never undone.
    browsing_asked: Mutex<HashSet<BrowsingFor>>,
    /// Held across one decision on whether to browse, setting read included,
    /// so the decision made last is made on the last change.
    browsing_decision: tokio::sync::Mutex<()>,
    /// Whether browsing runs: asked for while casting was on, and neither
    /// stopped nor turned off since. Changed under the discovery lock; a
    /// renderer a host reports is taken only while it holds.
    browsing: AtomicBool,
    /// How many casts and stops have been asked for. A cast is known by the
    /// count its ask made, and moves playback only while the count is still
    /// that: stopping casting, turning it off, or another cast counts past it
    /// and so calls it off. Playback is told of a cast or a stop under this
    /// channel's lock, so it hears them in the order they were asked.
    asked: tokio::sync::watch::Sender<u64>,
    /// The ephemeral server, once a cast has started one.
    server: Arc<Mutex<Option<EphemeralServer>>>,
    /// Where that server listens.
    serving: ServingNetwork,
    /// Where an AirPlay session's ids come from.
    ids: coven::IdSourceRef,
    /// What casting's background work — the server, the watchers, the
    /// discovery readers and the AirPlay stream threads — runs under, so a
    /// panic in it ends what it served.
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

/// A controller no one holds ends its tasks with it.
impl Drop for CastController {
    fn drop(&mut self) {
        let tasks = self.tasks.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner);
        for task in tasks.drain(..) {
            task.abort();
        }
    }
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
        ids: coven::IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Arc<Self> {
        Self::start_serving_on(services, runtime, discovery, ServingNetwork::Lan, ids, diagnostics)
    }

    /// [`Self::start`], with the ephemeral server on `serving`.
    fn start_serving_on(
        services: AppServices,
        runtime: Handle,
        discovery: RendererDiscovery,
        serving: ServingNetwork,
        ids: coven::IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Arc<Self> {
        let receivers = discovery.subscribe();
        let controller = Arc::new(Self {
            runtime: runtime.clone(),
            tasks: Mutex::new(Vec::new()),
            services,
            discovery: Mutex::new(discovery),
            browsing_asked: Mutex::new(HashSet::new()),
            browsing_decision: tokio::sync::Mutex::new(()),
            browsing: AtomicBool::new(false),
            asked: tokio::sync::watch::Sender::new(0),
            server: Arc::new(Mutex::new(None)),
            serving,
            ids,
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
            self.start_task(async move {
                while source.changed().await.is_ok() {
                    if changed.send(()).is_err() {
                        return;
                    }
                }
            });
        }
        drop(changed);
        let controller = Arc::downgrade(self);
        self.start_task(async move {
            loop {
                {
                    // Held only while a list is written, so the controller
                    // can end between changes.
                    let Some(controller) = controller.upgrade() else {
                        return;
                    };
                    let merged = controller.with_discovery(|discovery| {
                        caught_sync("merge the cast device lists", || discovery.devices())
                    });
                    match merged {
                        Ok(merged) => {
                            let listed = merged.iter().map(RendererDevice::receiver).collect();
                            if let Err(error) =
                                controller.services.show_cast_receivers(listed).await
                            {
                                controller
                                    .diagnostics
                                    .report_failure("list the cast receivers", &error);
                            }
                        }
                        Err(bug) => {
                            controller.diagnostics.panicked(bug);
                        }
                    }
                }
                if changes.recv().await.is_none() {
                    return;
                }
                while changes.try_recv().is_ok() {}
            }
        });
    }

    /// Follow the library's `cast_enabled` setting: each change decides
    /// browsing again, and turning casting off also ends any session in flight,
    /// so the machinery is idle rather than merely hidden. Turning it on browses
    /// only for a place still open. A panic applying one change is reported, and
    /// the next change is applied.
    fn spawn_setting_watcher(self: &Arc<Self>) {
        let setting = self.services.follow_preferences(|prefs| prefs.cast_enabled);
        let controller = Arc::downgrade(self);
        self.start_task(async move {
            let mut was_enabled = None;
            loop {
                match setting.next().await {
                    Ok(enabled) => {
                        // Held only while a change is applied, so the
                        // controller can end while the watcher waits.
                        let Some(controller) = controller.upgrade() else {
                            break;
                        };
                        let turned_off = was_enabled == Some(true) && !enabled;
                        was_enabled = Some(enabled);
                        let applied = caught("apply the casting setting", async {
                            controller.decide_browsing().await;
                            if turned_off {
                                controller.stop_casting();
                            }
                        })
                        .await;
                        if let Err(bug) = applied {
                            controller.diagnostics.panicked(bug);
                        }
                        #[cfg(test)]
                        controller.setting_applied.send_replace(Some(enabled));
                    }
                    Err(bae_store::live_query::LiveQueryError::Query(error)) => {
                        warn!("couldn't read whether casting is on: {error}");
                    }
                    Err(bae_store::live_query::LiveQueryError::Cancelled) => {
                        debug!("cast setting watcher stopped: the library closed, or its read ended on a bug");
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
        self.start_task(async move {
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

    /// Start `task` on the runtime in the controller's set, its panic
    /// reported as it happens. Tasks that ended since the last start leave
    /// the set here.
    fn start_task(&self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        let mut tasks = lock_or_crash(&self.diagnostics, "the casting tasks", &self.tasks);
        tasks.retain(|task| !task.is_finished());
        // Each task catches the panic of each step it takes; a panic outside
        // them ends that task, which the report is all there is to say of.
        tasks.push(self.diagnostics.spawn_reported(&self.runtime, "a casting task", task, |_| {}));
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

    /// The places open that list receivers.
    fn browsing_asked(&self) -> MutexGuard<'_, HashSet<BrowsingFor>> {
        lock_or_crash(&self.diagnostics, "the places asking to browse", &self.browsing_asked)
    }

    /// Browse exactly while a place asks and casting is on. The setting is the
    /// gate that keeps the network quiet, not what a place shows, and every
    /// change to either decides again. Decisions run one at a time, each on a
    /// fresh read, so one read before a change can't land after the decision
    /// that change made.
    async fn decide_browsing(&self) {
        let _turn = self.browsing_decision.lock().await;
        let enabled = match self.enabled().await {
            Ok(enabled) => enabled,
            Err(error) => {
                warn!("nothing browses: {error}");
                false
            }
        };
        self.with_discovery(|discovery| {
            if enabled && !self.browsing_asked().is_empty() {
                discovery.start(&self.diagnostics);
                self.browsing.store(true, Ordering::Release);
            } else {
                self.stop_browsing(discovery);
            }
        });
    }

    /// End the browse, under the discovery lock.
    fn stop_browsing(&self, discovery: &mut RendererDiscovery) {
        self.browsing.store(false, Ordering::Release);
        discovery.stop();
    }

    /// `asker` opened: browse for it. Idempotent, and a place that opens while
    /// a browse runs joins it. The setting is read off the caller's thread;
    /// browsing starts once it is, unless every place closed meanwhile.
    pub fn start_discovery(self: &Arc<Self>, asker: BrowsingFor) {
        self.browsing_asked().insert(asker);
        let controller = self.clone();
        self.start_task(async move {
            controller.decide_browsing().await;
            #[cfg(test)]
            controller.browsing_decided.send_modify(|decided| *decided += 1);
        });
    }

    /// `asker` closed: the browse ends with the last place open.
    pub fn stop_discovery(&self, asker: BrowsingFor) {
        self.browsing_asked().remove(&asker);
        self.with_discovery(|discovery| {
            if self.browsing_asked().is_empty() {
                self.stop_browsing(discovery);
            }
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
    /// the device fetches each track through. Calls off the cast asked for
    /// before it; stopping casting, turning it off, or a newer cast calls this
    /// one off while it connects, and it ends with [`CastError::CalledOff`]
    /// without moving playback.
    pub async fn cast_to(&self, device_id: &str) -> Result<(), CastError> {
        let mut asked = self.asked.subscribe();
        let mut number = 0;
        self.asked.send_modify(|count| {
            *count += 1;
            number = *count;
        });
        tokio::select! {
            biased;
            _ = asked.wait_for(|count| *count != number) => Err(CastError::CalledOff),
            started = self.connect_and_hand_over(device_id, number) => started,
        }
    }

    /// Move playback with `hand_over` while cast `number` is still the last
    /// thing asked for.
    fn hand_over(&self, number: u64, hand_over: impl FnOnce()) -> Result<(), CastError> {
        let mut handed = false;
        self.asked.send_if_modified(|count| {
            if *count == number {
                hand_over();
                handed = true;
            }
            false
        });
        if handed {
            Ok(())
        } else {
            Err(CastError::CalledOff)
        }
    }

    async fn connect_and_hand_over(&self, device_id: &str, number: u64) -> Result<(), CastError> {
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
            let sink =
                build_airplay_sink(&self.ids, &self.diagnostics, *addr, *port, capabilities)?;
            return self.hand_over(number, || {
                self.services.playback_play_on_airplay(sink, device.receiver());
            });
        }

        self.start_fetch_renderer(device, number).await
    }

    /// Build the control channel for a device, and name its flavor, which
    /// decides what it is served. Cast connects over the network (run off the runtime so it
    /// never stalls a runtime thread); UPnP has no handshake — each SOAP action
    /// is its own request.
    async fn start_fetch_renderer(
        &self,
        device: RendererDevice,
        number: u64,
    ) -> Result<(), CastError> {
        let (channel, flavor): (Box<dyn RendererChannel>, RendererFlavor) = match &device.connection
        {
            RendererConnection::Cast { addr, port } => {
                let (addr, port) = (*addr, *port);
                let channel = self
                    .diagnostics
                    .blocking("connect to the Cast device", move || {
                        RustCastChannel::connect(addr, port)
                    })
                    .await
                    .map_err(CastError::Internal)?
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

        self.hand_over(number, || {
            self.services.playback_play_on(
                channel,
                device.receiver(),
                RendererMediaSource::new(stream_provider, cover_provider, flavor),
            );
        })
    }

    /// Stop casting: call off a cast still connecting, and tell the playback
    /// service, which ends the session and announces the return to local; the
    /// status follower then stops the server. Under the lock a cast moves
    /// playback under, so no cast asked for before the stop lands after it.
    pub fn stop_casting(&self) {
        self.asked.send_modify(|count| {
            *count += 1;
            self.services.playback_stop_remote();
        });
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
    Arc::new(move |track_id: &str, format: &RendererStreamFormat| {
        let mut url = format!(
            "{base_url}/rest/stream?id={song}&{auth}&v=1.16.1&c=bae-cast",
            song = bae_subsonic::song_id(track_id),
            auth = auth_params(&credential),
        );
        match format {
            RendererStreamFormat::Raw(_) => url.push_str("&format=raw"),
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
    Arc::new(move |cover: &bae_records::ImageRef| {
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
