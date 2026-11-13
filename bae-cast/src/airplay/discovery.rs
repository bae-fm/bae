//! mDNS discovery of AirPlay receivers on the local network.
//!
//! `AirPlayDiscovery` browses both `_airplay._tcp.local.` (AirPlay 2 and newer
//! AirPlay 1 gear) and `_raop._tcp.local.` (legacy RAOP) and keeps a live list of
//! reachable receivers in a `PublishedDevices` — the same
//! start-with-the-picker / stop-when-it-closes shape as [`crate::cast`]
//! discovery.
//!
//! A single receiver often advertises both service types; the browse keys entries
//! by device id and prefers the AirPlay 2 advertisement, so the picker shows one
//! entry per receiver at the dialect it will actually be driven with. Turning the
//! [`AirPlayDevice`]s into the merged renderer-device list is the picker's job,
//! not this module's.

use std::collections::HashMap;
use std::net::IpAddr;

use bae_diagnostics::{Diagnostics, ReportedThread};
use bae_util::task_failure::caught_sync;
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tracing::{debug, warn};

use super::capabilities::{AirPlayCapabilities, Dialect, UnreadableSetting};
use crate::renderer::discovery::{sort_for_picker, RendererServiceType};
use crate::renderer::protocol_discovery::{
    self, preferred_address, stated, Browse, ProtocolDiscovery,
};
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::{RendererConnection, RendererDevice};

/// The two AirPlay service types browsed for.
const SERVICES: [AirPlayService; 2] = [AirPlayService::AirPlay, AirPlayService::Raop];

/// The two service types an AirPlay receiver advertises on. Cast has no place
/// here, which is why this is not a [`RendererServiceType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AirPlayService {
    /// `_airplay._tcp`: AirPlay 2 and newer AirPlay 1 gear.
    AirPlay,
    /// `_raop._tcp`: legacy RAOP.
    Raop,
}

impl AirPlayService {
    /// The service type as discovery names it.
    pub(crate) fn renderer_type(self) -> RendererServiceType {
        match self {
            Self::AirPlay => RendererServiceType::AirPlay,
            Self::Raop => RendererServiceType::Raop,
        }
    }

    /// The TXT record the service states the receiver's model in.
    fn model_record(self) -> &'static str {
        match self {
            Self::AirPlay => "model",
            Self::Raop => "am",
        }
    }
}

/// A discovered AirPlay receiver: enough to display it, reach its RTSP port, and
/// decide how to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AirPlayDevice {
    /// Stable receiver identifier — the device id (MAC) it advertises, as
    /// [`normalise_mac`] writes it, used to
    /// collapse the same receiver seen on both service types.
    pub id: String,
    /// Human-readable name for the picker.
    pub name: String,
    /// The model it states of itself, or `None`.
    pub model: Option<String>,
    /// The receiver's RTSP control address.
    pub addr: IpAddr,
    pub port: u16,
    /// What it announced about itself, or the setting it announced that bae
    /// can't read — then it can't be connected to.
    pub capabilities: Result<AirPlayCapabilities, UnreadableSetting>,
}

impl AirPlayDevice {
    /// Project into the merged renderer-device list the picker shows across
    /// flavors, carrying what the sender needs to open the connection.
    pub(crate) fn to_renderer_device(&self) -> RendererDevice {
        RendererDevice {
            id: self.id.clone(),
            name: self.name.clone(),
            model: self.model.clone(),
            connection: RendererConnection::AirPlay {
                addr: self.addr,
                port: self.port,
                capabilities: self.capabilities.clone(),
            },
        }
    }
}

/// Browses for AirPlay receivers and publishes the current list. Start and stop
/// with the picker's visibility; a stopped discovery holds no mDNS daemon and no
/// browse threads.
pub(crate) type AirPlayDiscovery = ProtocolDiscovery<AirPlayBrowse>;

/// A live AirPlay browse: the mDNS daemon and the threads draining its two
/// service types.
pub(crate) struct AirPlayBrowse {
    daemon: ServiceDaemon,
    readers: Vec<ReportedThread>,
}

impl Browse for AirPlayBrowse {
    fn start(devices: &PublishedDevices, diagnostics: &Diagnostics) -> Option<Self> {
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                warn!("airplay discovery: failed to start mDNS daemon: {e}");
                return None;
            }
        };
        devices.clear();

        // One shared table both service browses feed, behind a mutex, so a
        // receiver appearing on `_airplay._tcp` and `_raop._tcp` collapses to one
        // entry regardless of which browse resolves first.
        let table = std::sync::Arc::new(std::sync::Mutex::new(DeviceTable::default()));
        let mut readers = Vec::with_capacity(SERVICES.len());
        for service in SERVICES {
            let mdns_type = service.renderer_type().mdns_service_type();
            let events = match daemon.browse(mdns_type) {
                Ok(events) => events,
                Err(e) => {
                    warn!("airplay discovery: failed to browse {mdns_type}: {e}");
                    continue;
                }
            };
            let devices = devices.clone();
            let table = table.clone();
            let reader_diagnostics = diagnostics.clone();
            // A reader that panicked can no longer keep the list current, so
            // the list is emptied rather than left showing receivers it can't
            // confirm.
            let panicked_devices = devices.clone();
            let reader = diagnostics.spawn_thread_reported(
                "airplay browse reader",
                move || run_browse(service, events, table, devices, reader_diagnostics),
                move |_| panicked_devices.clear(),
            );
            match reader {
                Ok(reader) => readers.push(reader),
                Err(e) => warn!("airplay discovery: failed to start a browse reader thread: {e}"),
            }
        }
        if readers.is_empty() {
            if let Err(e) = daemon.shutdown() {
                debug!("airplay discovery: daemon shutdown after browse failures: {e}");
            }
            return None;
        }
        Some(Self { daemon, readers })
    }

    fn stop(self) {
        if let Err(e) = self.daemon.shutdown() {
            warn!("airplay discovery: mDNS daemon shutdown failed: {e}");
        }
        for reader in self.readers {
            reader.join();
        }
    }
}

/// The resolved receivers, keyed first by mDNS service fullname (so a
/// `ServiceRemoved` can drop the right entry) and collapsed to one entry per
/// device id when snapshotted.
#[derive(Default)]
struct DeviceTable {
    by_fullname: HashMap<String, AirPlayDevice>,
}

/// Drain one service type's mDNS event channel into the shared table, publishing
/// a de-duplicated snapshot on every change. Returns when the channel closes.
/// An event whose handling panics is reported and skipped; the browse reads
/// on.
fn run_browse(
    service: AirPlayService,
    events: mdns_sd::Receiver<ServiceEvent>,
    table: std::sync::Arc<std::sync::Mutex<DeviceTable>>,
    devices: PublishedDevices,
    diagnostics: Diagnostics,
) {
    while let Ok(event) = events.recv() {
        let followed = caught_sync("follow an AirPlay advertisement", || {
            if let Some(change) = table_change(service, event) {
                let mut table = lock_table(&table);
                if change.apply(&mut table) {
                    let mut snapshot: Vec<RendererDevice> =
                        snapshot(&table).iter().map(AirPlayDevice::to_renderer_device).collect();
                    sort_for_picker(&mut snapshot);
                    devices.publish(snapshot);
                }
            }
        });
        if let Err(bug) = followed {
            diagnostics.panicked(bug);
        }
    }
}

/// The table both service browses share. Each change to it is one map
/// operation, made after the advertisement is read, so a panic while it was
/// held never leaves it partway through a change, and it stands.
fn lock_table(table: &std::sync::Mutex<DeviceTable>) -> std::sync::MutexGuard<'_, DeviceTable> {
    table.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What one mDNS event does to the table, read before the table is locked.
enum TableChange {
    Resolved { fullname: String, device: AirPlayDevice },
    Removed { fullname: String },
}

impl TableChange {
    /// Make the change: whether the table changed.
    fn apply(self, table: &mut DeviceTable) -> bool {
        match self {
            Self::Resolved { fullname, device } => {
                table.by_fullname.insert(fullname, device);
                true
            }
            Self::Removed { fullname } => table.by_fullname.remove(&fullname).is_some(),
        }
    }
}

/// The change `event` makes, or `None` when it makes none the picker sees.
fn table_change(service: AirPlayService, event: ServiceEvent) -> Option<TableChange> {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let fullname = resolved.fullname.clone();
            match device_from_resolved(service, &resolved) {
                Some(device) => Some(TableChange::Resolved { fullname, device }),
                None => {
                    debug!("airplay discovery: ignoring unusable service {fullname}");
                    None
                }
            }
        }
        ServiceEvent::ServiceRemoved(_ty, fullname) => Some(TableChange::Removed { fullname }),
        _ => None,
    }
}

/// One entry per device id, preferring the AirPlay 2 advertisement when a
/// receiver is seen on both service types.
fn snapshot(table: &DeviceTable) -> Vec<AirPlayDevice> {
    dedupe_by_id(table.by_fullname.values())
}

/// One entry per receiver id, preferring the AirPlay 2 advertisement when a
/// receiver is seen on both service types — the same receiver reached the better
/// way, not two picker rows. An advertisement bae can't read wins over a
/// readable one: without it bae can't tell which dialect the receiver speaks
/// best, so the receiver shows as one it can't connect to. Whichever is chosen,
/// the receiver keeps the strictest access its advertisements state. Shared with the
/// reported-discovery path, which merges services the host's browser found the
/// same way.
pub(crate) fn dedupe_by_id<'a>(
    devices: impl Iterator<Item = &'a AirPlayDevice>,
) -> Vec<AirPlayDevice> {
    let devices: Vec<&AirPlayDevice> = devices.collect();
    let mut by_id: HashMap<&str, &AirPlayDevice> = HashMap::new();
    for &device in &devices {
        by_id
            .entry(&device.id)
            .and_modify(|existing| {
                let replaces = match (&device.capabilities, &existing.capabilities) {
                    (Err(_), Ok(_)) => true,
                    (Ok(new), Ok(old)) => {
                        matches!(new.dialect, Dialect::AirPlay2 { .. })
                            && matches!(old.dialect, Dialect::Raop { .. })
                    }
                    (Ok(_), Err(_)) | (Err(_), Err(_)) => false,
                };
                if replaces {
                    *existing = device;
                }
            })
            .or_insert(device);
    }
    by_id
        .into_values()
        .map(|chosen| {
            let mut chosen = chosen.clone();
            // `acl` and `act` are stated on the AirPlay advertisement only, so
            // the receiver keeps the strictest access any advertisement of it
            // states, whichever one is chosen.
            if let Ok(capabilities) = &mut chosen.capabilities {
                let strictest = devices
                    .iter()
                    .filter(|other| other.id == chosen.id)
                    .filter_map(|other| other.capabilities.as_ref().ok())
                    .map(|other| other.access)
                    .max();
                if let Some(strictest) = strictest {
                    capabilities.access = strictest;
                }
            }
            chosen
        })
        .collect()
}

/// Map a resolved mDNS service to an [`AirPlayDevice`]. The `ResolvedService`
/// type is opaque, so its pieces are pulled out here and mapped by the pure
/// [`map_device`], which the tests drive directly.
fn device_from_resolved(
    service: AirPlayService,
    resolved: &ResolvedService,
) -> Option<AirPlayDevice> {
    let txt = resolved
        .txt_properties
        .iter()
        .map(|p| (p.key().to_ascii_lowercase(), p.val_str().to_string()))
        .collect::<HashMap<String, String>>();
    map_device(
        service,
        instance_label(&resolved.fullname),
        resolved.port,
        resolved.addresses.iter().map(|scoped| scoped.to_ip_addr()),
        &txt,
    )
}

/// Build an [`AirPlayDevice`] from a service's fields, or `None` if it lacks a
/// reachable address. IPv4 is preferred (AirPlay receivers advertise it and the
/// RTSP control connection is IPv4 LAN). The device id is the advertised
/// `deviceid`, falling back to the MAC prefix of a RAOP instance name
/// (`AABBCCDDEEFF@Name`), either as a [`normalise_mac`] MAC, then to the
/// instance label as it is.
pub(crate) fn map_device(
    service: AirPlayService,
    instance: &str,
    port: u16,
    addresses: impl Iterator<Item = IpAddr>,
    txt: &HashMap<String, String>,
) -> Option<AirPlayDevice> {
    let id = txt
        .get("deviceid")
        .map(String::as_str)
        .or_else(|| raop_mac_prefix(instance))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(normalise_mac)
        .unwrap_or_else(|| instance.to_string());

    let name = raop_name_suffix(instance).unwrap_or(instance).trim().to_string();
    let name = if name.is_empty() { id.clone() } else { name };

    let addr = preferred_address(addresses)?;
    let capabilities = AirPlayCapabilities::from_txt(service.renderer_type(), txt);
    if let Err(unreadable) = &capabilities {
        warn!(
            "airplay discovery: {name} ({id}) on {service:?} advertises a setting bae \
             can't read, so it can't be connected to: {unreadable}"
        );
    }
    // `_airplay._tcp` states the model as `model`, `_raop._tcp` as `am`.
    let model = stated(txt.get(service.model_record()).map(String::as_str));
    Some(AirPlayDevice { id, name, model, addr, port, capabilities })
}

/// A MAC as one id however it was written: `_airplay._tcp` states `deviceid` as
/// `a4:83:e7:71:d1:0d`, a RAOP instance name as `A483E771D10D`, and the same
/// receiver must get the same id from both.
fn normalise_mac(mac: &str) -> String {
    mac.chars().filter(|c| *c != ':').map(|c| c.to_ascii_uppercase()).collect()
}

/// The instance label of a service fullname (`"Living Room._airplay._tcp.local."`
/// → `"Living Room"`).
fn instance_label(fullname: &str) -> &str {
    protocol_discovery::instance_label(fullname, &["._airplay", "._raop"])
}

/// The MAC prefix of a RAOP instance name `AABBCCDDEEFF@Friendly Name`, or `None`
/// when the instance isn't in that form.
fn raop_mac_prefix(instance: &str) -> Option<&str> {
    let (prefix, _) = instance.split_once('@')?;
    (prefix.len() == 12 && prefix.bytes().all(|b| b.is_ascii_hexdigit())).then_some(prefix)
}

/// The friendly-name suffix of a RAOP instance name `MAC@Friendly Name`.
fn raop_name_suffix(instance: &str) -> Option<&str> {
    let (prefix, name) = instance.split_once('@')?;
    (prefix.len() == 12 && prefix.bytes().all(|b| b.is_ascii_hexdigit())).then_some(name)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
