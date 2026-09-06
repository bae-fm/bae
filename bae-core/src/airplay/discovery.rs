//! mDNS discovery of AirPlay receivers on the local network.
//!
//! [`AirPlayDiscovery`] browses both `_airplay._tcp.local.` (AirPlay 2 and newer
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
use std::thread::JoinHandle;

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tracing::{debug, warn};

use super::capabilities::{AirPlayCapabilities, Dialect};
use crate::renderer::discovery::RendererServiceType;
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::{RendererConnection, RendererDevice};

/// The two AirPlay service types browsed for.
const SERVICE_TYPES: [RendererServiceType; 2] =
    [RendererServiceType::AirPlay, RendererServiceType::Raop];

/// A discovered AirPlay receiver: enough to display it, reach its RTSP port, and
/// decide how to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirPlayDevice {
    /// Stable receiver identifier — the device id (MAC) it advertises, used to
    /// collapse the same receiver seen on both service types.
    pub id: String,
    /// Human-readable name for the picker.
    pub name: String,
    /// The receiver's RTSP control address.
    pub addr: IpAddr,
    pub port: u16,
    pub capabilities: AirPlayCapabilities,
}

impl AirPlayDevice {
    /// Project into the merged renderer-device list the picker shows across
    /// flavors, carrying what the sender needs to open the connection.
    pub fn to_renderer_device(&self) -> RendererDevice {
        RendererDevice {
            id: self.id.clone(),
            name: self.name.clone(),
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
pub struct AirPlayDiscovery {
    devices: PublishedDevices,
    running: Option<Running>,
}

struct Running {
    daemon: ServiceDaemon,
    readers: Vec<JoinHandle<()>>,
}

impl AirPlayDiscovery {
    pub fn new() -> Self {
        Self {
            devices: PublishedDevices::new(),
            running: None,
        }
    }

    /// Subscribe to the live device list — merged renderer devices, so the
    /// picker's one forwarder handles AirPlay like Cast and UPnP. The current
    /// snapshot is available immediately on the returned receiver.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Vec<RendererDevice>> {
        self.devices.subscribe()
    }

    /// The current device list snapshot.
    pub fn devices(&self) -> Vec<RendererDevice> {
        self.devices.current()
    }

    /// Whether an mDNS daemon and browse threads are live right now. For tests
    /// that assert browsing is (or is not) reaching the network.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_browsing(&self) -> bool {
        self.running.is_some()
    }

    /// Begin browsing both service types. Idempotent: a second call while already
    /// browsing is a no-op.
    pub fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                warn!("airplay discovery: failed to start mDNS daemon: {e}");
                return;
            }
        };
        self.devices.clear();

        // One shared table both service browses feed, behind a mutex, so a
        // receiver appearing on `_airplay._tcp` and `_raop._tcp` collapses to one
        // entry regardless of which browse resolves first.
        let table = std::sync::Arc::new(std::sync::Mutex::new(DeviceTable::default()));
        let mut readers = Vec::with_capacity(SERVICE_TYPES.len());
        for service_type in SERVICE_TYPES {
            let mdns_type = service_type.mdns_service_type();
            let events = match daemon.browse(mdns_type) {
                Ok(events) => events,
                Err(e) => {
                    warn!("airplay discovery: failed to browse {mdns_type}: {e}");
                    continue;
                }
            };
            let devices = self.devices.clone();
            let table = table.clone();
            readers.push(std::thread::spawn(move || {
                run_browse(service_type, events, table, devices);
            }));
        }
        if readers.is_empty() {
            if let Err(e) = daemon.shutdown() {
                debug!("airplay discovery: daemon shutdown after browse failures: {e}");
            }
            return;
        }
        self.running = Some(Running { daemon, readers });
    }

    /// Stop browsing and release the mDNS daemon. The last published device list
    /// is kept.
    pub fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        if let Err(e) = running.daemon.shutdown() {
            warn!("airplay discovery: mDNS daemon shutdown failed: {e}");
        }
        for reader in running.readers {
            if reader.join().is_err() {
                warn!("airplay discovery: a browse reader thread panicked");
            }
        }
    }
}

impl Default for AirPlayDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AirPlayDiscovery {
    fn drop(&mut self) {
        self.stop();
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
fn run_browse(
    service_type: RendererServiceType,
    events: mdns_sd::Receiver<ServiceEvent>,
    table: std::sync::Arc<std::sync::Mutex<DeviceTable>>,
    devices: PublishedDevices,
) {
    while let Ok(event) = events.recv() {
        let changed = {
            let mut table = table.lock().unwrap();
            match event {
                ServiceEvent::ServiceResolved(resolved) => {
                    let fullname = resolved.fullname.clone();
                    match device_from_resolved(service_type, &resolved) {
                        Some(device) => {
                            table.by_fullname.insert(fullname, device);
                            true
                        }
                        None => {
                            debug!("airplay discovery: ignoring unusable service {fullname}");
                            false
                        }
                    }
                }
                ServiceEvent::ServiceRemoved(_ty, fullname) => {
                    table.by_fullname.remove(&fullname).is_some()
                }
                _ => false,
            }
        };
        if !changed {
            continue;
        }
        devices.publish(
            snapshot(&table.lock().unwrap())
                .iter()
                .map(AirPlayDevice::to_renderer_device)
                .collect(),
        );
    }
}

/// One entry per device id, preferring the AirPlay 2 advertisement when a
/// receiver is seen on both service types, sorted by name for a stable UI.
fn snapshot(table: &DeviceTable) -> Vec<AirPlayDevice> {
    let mut devices = dedupe_by_id(table.by_fullname.values());
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    devices
}

/// One entry per receiver id, preferring the AirPlay 2 advertisement when a
/// receiver is seen on both service types — the same receiver reached the better
/// way, not two picker rows. Shared with the reported-discovery path, which
/// merges services the host's browser found the same way.
pub(crate) fn dedupe_by_id<'a>(
    devices: impl Iterator<Item = &'a AirPlayDevice>,
) -> Vec<AirPlayDevice> {
    let mut by_id: HashMap<&str, &AirPlayDevice> = HashMap::new();
    for device in devices {
        by_id
            .entry(&device.id)
            .and_modify(|existing| {
                if device.capabilities.dialect == Dialect::AirPlay2
                    && existing.capabilities.dialect == Dialect::Raop
                {
                    *existing = device;
                }
            })
            .or_insert(device);
    }
    by_id.into_values().cloned().collect()
}

/// Map a resolved mDNS service to an [`AirPlayDevice`]. The `ResolvedService`
/// type is opaque, so its pieces are pulled out here and mapped by the pure
/// [`map_device`], which the tests drive directly.
fn device_from_resolved(
    service_type: RendererServiceType,
    resolved: &ResolvedService,
) -> Option<AirPlayDevice> {
    let txt = resolved
        .txt_properties
        .iter()
        .map(|p| (p.key().to_ascii_lowercase(), p.val_str().to_string()))
        .collect::<HashMap<String, String>>();
    map_device(
        service_type,
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
/// (`AABBCCDDEEFF@Name`), then to the instance label.
pub(crate) fn map_device(
    service_type: RendererServiceType,
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
        .unwrap_or(instance)
        .to_string();

    let name = raop_name_suffix(instance)
        .unwrap_or(instance)
        .trim()
        .to_string();
    let name = if name.is_empty() { id.clone() } else { name };

    let mut chosen: Option<IpAddr> = None;
    for addr in addresses {
        if addr.is_ipv4() {
            chosen = Some(addr);
            break;
        }
        chosen.get_or_insert(addr);
    }

    Some(AirPlayDevice {
        id,
        name,
        addr: chosen?,
        port,
        capabilities: AirPlayCapabilities::from_txt(service_type, txt),
    })
}

/// The instance label of a service fullname (`"Living Room._airplay._tcp.local."`
/// → `"Living Room"`).
fn instance_label(fullname: &str) -> &str {
    fullname
        .split_once("._airplay")
        .or_else(|| fullname.split_once("._raop"))
        .map(|(instance, _)| instance)
        .unwrap_or(fullname)
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
mod tests {
    use super::*;

    fn txt(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::from([a, b, c, d])
    }

    /// A features string with bit 48 (CoreUtils pairing/encryption) set — the
    /// AirPlay 2 marker. Bit 48 is bit 16 of the high word: `0xLOW,0xHIGH` =
    /// `0x0,0x10000`.
    const AIRPLAY2_FEATURES: &str = "0x0,0x10000";

    /// An `_airplay._tcp` service maps to an AirPlay 2 device named by its
    /// instance label, addressed at its IPv4 address.
    #[test]
    fn airplay2_service_maps_to_device() {
        let device = map_device(
            RendererServiceType::AirPlay,
            "Kitchen",
            7000,
            [v4(10, 0, 0, 4)].into_iter(),
            &txt(&[
                ("deviceid", "AA:BB:CC:DD:EE:FF"),
                ("features", AIRPLAY2_FEATURES),
            ]),
        )
        .expect("a service with an address maps");
        assert_eq!(device.id, "AA:BB:CC:DD:EE:FF");
        assert_eq!(device.name, "Kitchen");
        assert_eq!(device.addr, v4(10, 0, 0, 4));
        assert_eq!(device.port, 7000);
        assert_eq!(device.capabilities.dialect, Dialect::AirPlay2);
    }

    /// A RAOP instance name `MAC@Name` yields the MAC as the id and the suffix as
    /// the name.
    #[test]
    fn raop_instance_name_splits_mac_and_name() {
        let device = map_device(
            RendererServiceType::Raop,
            "001122334455@Studio Monitor",
            5000,
            [v4(10, 0, 0, 9)].into_iter(),
            &txt(&[("et", "0,1"), ("cn", "0,1")]),
        )
        .expect("maps");
        assert_eq!(device.id, "001122334455");
        assert_eq!(device.name, "Studio Monitor");
        assert_eq!(device.capabilities.dialect, Dialect::Raop);
    }

    /// A service with no resolvable address is dropped.
    #[test]
    fn service_without_address_is_dropped() {
        assert!(map_device(
            RendererServiceType::AirPlay,
            "Nowhere",
            7000,
            std::iter::empty(),
            &txt(&[("deviceid", "X")]),
        )
        .is_none());
    }

    /// IPv4 is chosen over an IPv6 address advertised for the same service.
    #[test]
    fn ipv4_is_preferred() {
        let device = map_device(
            RendererServiceType::AirPlay,
            "Dual",
            7000,
            ["fe80::1".parse().unwrap(), v4(192, 168, 1, 5)].into_iter(),
            &txt(&[("deviceid", "ID")]),
        )
        .unwrap();
        assert_eq!(device.addr, v4(192, 168, 1, 5));
    }

    /// A receiver seen on both `_raop._tcp` and `_airplay._tcp` collapses to one
    /// entry, kept at the AirPlay 2 dialect.
    #[test]
    fn same_receiver_on_both_services_prefers_airplay2() {
        let raop = map_device(
            RendererServiceType::Raop,
            "AABBCCDDEEFF@Den",
            5000,
            [v4(10, 0, 0, 7)].into_iter(),
            &txt(&[("et", "1"), ("cn", "1")]),
        )
        .unwrap();
        let airplay = map_device(
            RendererServiceType::AirPlay,
            "Den",
            7000,
            [v4(10, 0, 0, 7)].into_iter(),
            &txt(&[
                ("deviceid", "AABBCCDDEEFF"),
                ("features", AIRPLAY2_FEATURES),
            ]),
        )
        .unwrap();

        let mut table = DeviceTable::default();
        table
            .by_fullname
            .insert("AABBCCDDEEFF@Den._raop._tcp.local.".to_string(), raop);
        table
            .by_fullname
            .insert("Den._airplay._tcp.local.".to_string(), airplay);

        let snap = snapshot(&table);
        assert_eq!(snap.len(), 1, "one entry per receiver");
        assert_eq!(snap[0].capabilities.dialect, Dialect::AirPlay2);
        assert_eq!(snap[0].port, 7000);
    }
}
