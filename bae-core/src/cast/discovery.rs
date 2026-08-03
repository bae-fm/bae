//! mDNS discovery of Cast devices on the local network.
//!
//! [`CastDiscovery`] browses `_googlecast._tcp.local.` and keeps a live list of
//! reachable devices, published over a [`tokio::sync::watch`] channel. Browsing
//! is not always-on: the caller (the device-picker UI) starts it when the
//! picker opens and stops it when it closes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::thread::JoinHandle;

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tracing::{debug, warn};

use crate::renderer::{RendererConnection, RendererDevice};

/// The Cast service type browsed for.
const CAST_SERVICE_TYPE: &str = "_googlecast._tcp.local.";

/// Browses for Cast devices and publishes the current list over a watch channel,
/// as [`RendererDevice`]s so the picker shows one merged list. Start and stop
/// with the picker's visibility; a stopped discovery holds no mDNS daemon and no
/// browse thread.
pub struct CastDiscovery {
    devices_tx: tokio::sync::watch::Sender<Vec<RendererDevice>>,
    devices_rx: tokio::sync::watch::Receiver<Vec<RendererDevice>>,
    /// The running browse: the mDNS daemon and the thread draining its events.
    /// `None` while stopped.
    running: Option<Running>,
}

struct Running {
    daemon: ServiceDaemon,
    reader: JoinHandle<()>,
}

impl CastDiscovery {
    pub fn new() -> Self {
        let (devices_tx, devices_rx) = tokio::sync::watch::channel(Vec::new());
        Self {
            devices_tx,
            devices_rx,
            running: None,
        }
    }

    /// Subscribe to the live device list. The current snapshot is available
    /// immediately on the returned receiver.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Vec<RendererDevice>> {
        self.devices_rx.clone()
    }

    /// The current device list snapshot.
    pub fn devices(&self) -> Vec<RendererDevice> {
        self.devices_rx.borrow().clone()
    }

    /// Whether an mDNS daemon and browse thread are live right now. For tests
    /// that assert browsing is (or is not) reaching the network.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_browsing(&self) -> bool {
        self.running.is_some()
    }

    /// Begin browsing. Idempotent: a second call while already browsing is a
    /// no-op. Publishes list updates over the watch channel as devices come and
    /// go.
    pub fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                warn!("cast discovery: failed to start mDNS daemon: {e}");
                return;
            }
        };
        let events = match daemon.browse(CAST_SERVICE_TYPE) {
            Ok(events) => events,
            Err(e) => {
                warn!("cast discovery: failed to browse {CAST_SERVICE_TYPE}: {e}");
                if let Err(shutdown_err) = daemon.shutdown() {
                    debug!(
                        "cast discovery: mDNS daemon shutdown after browse failure: {shutdown_err}"
                    );
                }
                return;
            }
        };
        // Reset to an empty list at the start of a fresh browse so a stale
        // snapshot from a previous session isn't shown before the first event.
        // `send_replace` writes the value with no Result to swallow (a watch
        // sender is only "closed" when every receiver drops, but this holds one).
        self.devices_tx.send_replace(Vec::new());
        let devices_tx = self.devices_tx.clone();
        let reader = std::thread::spawn(move || run_browse(events, devices_tx));
        self.running = Some(Running { daemon, reader });
    }

    /// Stop browsing and release the mDNS daemon. The last published device list
    /// is kept on the watch channel.
    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        // Shutting the daemon down closes the event channel, so the reader
        // thread's loop ends and it can be joined.
        if let Err(e) = running.daemon.shutdown() {
            warn!("cast discovery: mDNS daemon shutdown failed: {e}");
        }
        if running.reader.join().is_err() {
            warn!("cast discovery: browse reader thread panicked");
        }
    }
}

impl Default for CastDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CastDiscovery {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Drain the mDNS event channel, maintaining the device list keyed by service
/// fullname, and publish a de-duplicated snapshot on every change. Returns when
/// the channel closes (the daemon shut down).
fn run_browse(
    events: mdns_sd::Receiver<ServiceEvent>,
    devices_tx: tokio::sync::watch::Sender<Vec<RendererDevice>>,
) {
    // Keyed by service fullname (what `ServiceRemoved` carries), so a device can
    // be dropped when it leaves.
    let mut by_fullname: HashMap<String, RendererDevice> = HashMap::new();
    while let Ok(event) = events.recv() {
        match event {
            ServiceEvent::ServiceResolved(resolved) => {
                let fullname = resolved.fullname.clone();
                match device_from_resolved(&resolved) {
                    Some(device) => {
                        by_fullname.insert(fullname, device);
                    }
                    None => {
                        debug!("cast discovery: ignoring unusable service {fullname}");
                        continue;
                    }
                }
            }
            ServiceEvent::ServiceRemoved(_ty, fullname) => {
                if by_fullname.remove(&fullname).is_none() {
                    continue;
                }
            }
            // Search lifecycle and pre-resolution "found" events carry no
            // address, and `ServiceEvent` is non-exhaustive, so anything else is
            // ignored until it's handled above.
            _ => continue,
        }
        if devices_tx.send(snapshot(&by_fullname)).is_err() {
            // Every receiver dropped; no one is watching. Keep draining so the
            // daemon's channel doesn't back up until shutdown closes it.
            debug!("cast discovery: no watchers for the device list");
        }
    }
}

/// A stable, de-duplicated device list: one entry per device id (a device seen
/// on several addresses collapses to one), sorted by name for a stable UI.
fn snapshot(by_fullname: &HashMap<String, RendererDevice>) -> Vec<RendererDevice> {
    let mut by_id: HashMap<&str, RendererDevice> = HashMap::new();
    for device in by_fullname.values() {
        by_id.entry(&device.id).or_insert_with(|| device.clone());
    }
    let mut devices: Vec<RendererDevice> = by_id.into_values().collect();
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    devices
}

/// Map a resolved mDNS service to a Cast [`RendererDevice`]. The `ResolvedService` type
/// is opaque (non-exhaustive), so the pieces are pulled out here and mapped by
/// the pure [`map_device`], which the tests drive directly.
fn device_from_resolved(resolved: &ResolvedService) -> Option<RendererDevice> {
    map_device(
        &resolved.fullname,
        resolved.port,
        resolved.addresses.iter().map(|scoped| scoped.to_ip_addr()),
        resolved.txt_properties.get_property_val_str("id"),
        resolved.txt_properties.get_property_val_str("fn"),
    )
}

/// Build a Cast [`RendererDevice`] from a service's fields, or `None` if it
/// lacks what's needed to reach and name it: a device id, and at least one
/// address. IPv4 is preferred — Cast receivers advertise it and the media
/// receiver fetches over IPv4 LAN. The name falls back to the service instance
/// label when a device advertises no `fn` value.
pub(super) fn map_device(
    fullname: &str,
    port: u16,
    addresses: impl Iterator<Item = IpAddr>,
    id: Option<&str>,
    friendly_name: Option<&str>,
) -> Option<RendererDevice> {
    let id = id.map(str::trim).filter(|id| !id.is_empty())?;
    let name = friendly_name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| instance_name(fullname));

    let mut chosen: Option<IpAddr> = None;
    for addr in addresses {
        if addr.is_ipv4() {
            chosen = Some(addr);
            break;
        }
        chosen.get_or_insert(addr);
    }

    Some(RendererDevice {
        id: id.to_string(),
        name,
        connection: RendererConnection::Cast {
            addr: chosen?,
            port,
        },
    })
}

/// The instance label of a service fullname (`"Living Room._googlecast._tcp.local."`
/// → `"Living Room"`), for the rare device that advertises no `fn` TXT value.
fn instance_name(fullname: &str) -> String {
    fullname
        .split_once("._googlecast")
        .map(|(instance, _)| instance)
        .unwrap_or(fullname)
        .to_string()
}
