//! Where the device picker's list comes from.
//!
//! bae finds renderers two ways, and a host runs exactly one of them:
//!
//! - [`BuiltinDiscovery`] — bae browses the network itself: mDNS for Cast and
//!   AirPlay, SSDP for UPnP. This is what desktop and Android run.
//! - [`ReportedDiscovery`] — the host's own service browser finds the services
//!   and reports each one here. iOS runs this, because joining a multicast group
//!   from the app's own socket needs an entitlement Apple grants by application,
//!   while the system's Bonjour browser needs only a declared service list. It
//!   covers the DNS-SD flavors only, so a host on this path sees Cast and
//!   AirPlay receivers and no UPnP ones (UPnP is found by SSDP, which is the
//!   same multicast bae may not send).
//!
//! Both publish the same merged [`RendererDevice`] list through a
//! `PublishedDevices` and are started and stopped with the picker's visibility.
//! The mapping from an advertised service to a device — TXT record parsing and
//! all — is the same code on both paths ([`crate::cast::discovery`],
//! [`crate::airplay::discovery`]); only who reads the network differs.

use std::collections::HashMap;
use std::net::IpAddr;

use tracing::debug;

use crate::airplay::discovery::AirPlayDevice;
use crate::airplay::AirPlayDiscovery;
use crate::cast::CastDiscovery;
use crate::dlna::DlnaDiscovery;
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::RendererDevice;

/// A DNS-SD service type a renderer advertises itself on. Names which mapping a
/// reported service goes through, and which type a host browser must browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererServiceType {
    /// Google Cast receivers.
    GoogleCast,
    /// AirPlay 2 and newer AirPlay 1 receivers.
    AirPlay,
    /// Legacy RAOP (AirPlay 1) receivers.
    Raop,
}

/// Every service type a renderer is found on. A host that browses for bae walks
/// this list rather than naming service types itself.
pub const RENDERER_SERVICE_TYPES: [RendererServiceType; 3] = [
    RendererServiceType::GoogleCast,
    RendererServiceType::AirPlay,
    RendererServiceType::Raop,
];

impl RendererServiceType {
    /// The DNS-SD service type to browse, without a domain — the form a host
    /// browser and an app's declared-services list take.
    pub fn dns_sd_type(self) -> &'static str {
        match self {
            Self::GoogleCast => "_googlecast._tcp",
            Self::AirPlay => "_airplay._tcp",
            Self::Raop => "_raop._tcp",
        }
    }

    /// The fully-qualified type bae's own mDNS browse subscribes to.
    pub(crate) fn mdns_service_type(self) -> &'static str {
        match self {
            Self::GoogleCast => "_googlecast._tcp.local.",
            Self::AirPlay => "_airplay._tcp.local.",
            Self::Raop => "_raop._tcp.local.",
        }
    }
}

/// One service a host browser resolved, as it comes off the wire: the service
/// type it was found on, the instance name it advertises, where it answers, and
/// its TXT record. Turning this into a [`RendererDevice`] — which key names the
/// device, which names it, what its TXT bits mean — happens here, not in the
/// host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedRenderer {
    pub service_type: RendererServiceType,
    /// The service instance name (`Living Room`), which is also the identity a
    /// later [`ReportedDiscovery::forget`] names.
    pub instance_name: String,
    /// The resolved address, in text form as the host's resolver renders it.
    pub addr: String,
    pub port: u16,
    /// The service's TXT record. Keys are matched case-insensitively.
    pub txt: HashMap<String, String>,
}

/// bae's own network browsing: the three protocol discoveries running side by
/// side, their device lists merged into one for the picker — a speaker is a
/// speaker, whatever its protocol.
pub struct BuiltinDiscovery {
    cast: CastDiscovery,
    dlna: DlnaDiscovery,
    airplay: AirPlayDiscovery,
}

impl BuiltinDiscovery {
    pub fn new() -> Self {
        Self {
            cast: CastDiscovery::new(),
            dlna: DlnaDiscovery::new(),
            airplay: AirPlayDiscovery::new(),
        }
    }
}

impl Default for BuiltinDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// One resolved service, at the type its mapping produced.
#[derive(Debug, Clone)]
enum ReportedEntry {
    Cast(RendererDevice),
    AirPlay(AirPlayDevice),
}

/// The devices a host browser has reported. Holds no socket of its own: the host
/// finds the services and pushes each one in.
pub struct ReportedDiscovery {
    devices: PublishedDevices,
    /// What the host has reported this browse, keyed by service type and
    /// instance name — the identity it reports a loss by. `None` while not
    /// browsing, so a report outside a browse accumulates nothing.
    reported: Option<HashMap<(RendererServiceType, String), ReportedEntry>>,
}

impl ReportedDiscovery {
    pub fn new() -> Self {
        Self {
            devices: PublishedDevices::new(),
            reported: None,
        }
    }

    /// Take a service the host resolved. Ignored when the host reports outside a
    /// browse, and when the service carries too little to reach or name the
    /// device.
    pub fn report(&mut self, service: ReportedRenderer) {
        let Some(reported) = self.reported.as_mut() else {
            debug!(
                "reported discovery: ignoring {} while not browsing",
                service.instance_name
            );
            return;
        };
        let key = (service.service_type, service.instance_name.clone());
        let Some(entry) = map_reported(&service) else {
            debug!(
                "reported discovery: ignoring unusable service {}",
                service.instance_name
            );
            return;
        };
        reported.insert(key, entry);
        self.publish();
    }

    /// Drop a service the host's browser no longer sees.
    pub fn forget(&mut self, service_type: RendererServiceType, instance_name: &str) {
        let Some(reported) = self.reported.as_mut() else {
            return;
        };
        if reported
            .remove(&(service_type, instance_name.to_string()))
            .is_none()
        {
            return;
        }
        self.publish();
    }

    fn publish(&self) {
        self.devices.publish(snapshot(
            self.reported.as_ref().expect("published while browsing"),
        ));
    }
}

impl Default for ReportedDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a reported service through the same code bae's own browse uses, or `None`
/// when it lacks what's needed to reach and name the device.
fn map_reported(service: &ReportedRenderer) -> Option<ReportedEntry> {
    let addr: IpAddr = service
        .addr
        .parse()
        .map_err(|e| {
            debug!(
                "reported discovery: unparseable address {} for {}: {e}",
                service.addr, service.instance_name
            );
        })
        .ok()?;
    let txt: HashMap<String, String> = service
        .txt
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
        .collect();
    match service.service_type {
        RendererServiceType::GoogleCast => crate::cast::discovery::map_device(
            &service.instance_name,
            service.port,
            std::iter::once(addr),
            txt.get("id").map(String::as_str),
            txt.get("fn").map(String::as_str),
        )
        .map(ReportedEntry::Cast),
        RendererServiceType::AirPlay | RendererServiceType::Raop => {
            crate::airplay::discovery::map_device(
                service.service_type,
                &service.instance_name,
                service.port,
                std::iter::once(addr),
                &txt,
            )
            .map(ReportedEntry::AirPlay)
        }
    }
}

/// The merged picker list: Cast devices collapsed by device id, AirPlay
/// receivers collapsed by receiver id (a receiver reported on both AirPlay
/// service types is one entry), sorted by name for a stable picker.
fn snapshot(
    reported: &HashMap<(RendererServiceType, String), ReportedEntry>,
) -> Vec<RendererDevice> {
    let cast = reported.values().filter_map(|entry| match entry {
        ReportedEntry::Cast(device) => Some(device),
        ReportedEntry::AirPlay(_) => None,
    });
    let airplay = reported.values().filter_map(|entry| match entry {
        ReportedEntry::AirPlay(device) => Some(device),
        ReportedEntry::Cast(_) => None,
    });
    let mut devices = crate::cast::discovery::dedupe_by_id(cast);
    devices.extend(
        crate::airplay::discovery::dedupe_by_id(airplay)
            .iter()
            .map(AirPlayDevice::to_renderer_device),
    );
    sort_for_picker(&mut devices);
    devices
}

/// One entry per device, sorted by name with the id breaking ties, so the picker
/// order is stable across requeries.
pub(crate) fn sort_for_picker(devices: &mut [RendererDevice]) {
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
}

/// The picker's device source. A host builds the one it can run
/// ([`RendererDiscovery::for_host`]) and everything above it — the cast
/// controller, the bridge, the picker — is the same on every platform.
pub enum RendererDiscovery {
    Builtin(BuiltinDiscovery),
    Reported(ReportedDiscovery),
}

impl RendererDiscovery {
    /// bae browses the network itself.
    pub fn builtin() -> Self {
        Self::Builtin(BuiltinDiscovery::new())
    }

    /// The host's browser finds the services and reports them in.
    pub fn reported() -> Self {
        Self::Reported(ReportedDiscovery::new())
    }

    /// The discovery this host can run. bae browses the network itself
    /// everywhere except iOS, where an app socket may not join a multicast group
    /// without an entitlement Apple grants by application; there the system's
    /// Bonjour browser finds the services and the app reports them in.
    pub fn for_host() -> Self {
        #[cfg(target_os = "ios")]
        {
            Self::reported()
        }
        #[cfg(not(target_os = "ios"))]
        {
            Self::builtin()
        }
    }

    /// Every list to watch for changes — one per underlying browse — so a caller
    /// can invalidate an open picker as devices come and go.
    pub fn subscribe(&self) -> Vec<tokio::sync::watch::Receiver<Vec<RendererDevice>>> {
        match self {
            Self::Builtin(builtin) => vec![
                builtin.cast.subscribe(),
                builtin.dlna.subscribe(),
                builtin.airplay.subscribe(),
            ],
            Self::Reported(reported) => vec![reported.devices.subscribe()],
        }
    }

    /// Begin browsing (the picker opened). Idempotent.
    pub fn start(&mut self) {
        match self {
            Self::Builtin(builtin) => {
                builtin.cast.start();
                builtin.dlna.start();
                builtin.airplay.start();
            }
            Self::Reported(reported) => {
                if reported.reported.is_some() {
                    return;
                }
                reported.reported = Some(HashMap::new());
                reported.devices.clear();
            }
        }
    }

    /// Stop browsing (the picker closed). The last published list is kept, as it
    /// is on the builtin path.
    pub fn stop(&mut self) {
        match self {
            Self::Builtin(builtin) => {
                builtin.cast.stop();
                builtin.dlna.stop();
                builtin.airplay.stop();
            }
            Self::Reported(reported) => reported.reported = None,
        }
    }

    /// The current merged device list.
    pub fn devices(&self) -> Vec<RendererDevice> {
        match self {
            Self::Builtin(builtin) => {
                let mut devices = builtin.cast.devices();
                devices.extend(builtin.dlna.devices());
                devices.extend(builtin.airplay.devices());
                sort_for_picker(&mut devices);
                devices
            }
            Self::Reported(reported) => reported.devices.current(),
        }
    }

    /// Take a service a host browser resolved. Only the reported path has a host
    /// browser; a builtin one reads the network itself and has nothing to do
    /// with a report.
    pub fn report(&mut self, service: ReportedRenderer) {
        match self {
            Self::Builtin(_) => debug!(
                "ignoring reported renderer {}: this host browses the network itself",
                service.instance_name
            ),
            Self::Reported(reported) => reported.report(service),
        }
    }

    /// Drop a service a host browser no longer sees.
    pub fn forget(&mut self, service_type: RendererServiceType, instance_name: &str) {
        match self {
            Self::Builtin(_) => debug!(
                "ignoring lost renderer {instance_name}: this host browses the network itself"
            ),
            Self::Reported(reported) => reported.forget(service_type, instance_name),
        }
    }

    /// Whether a browse is live right now. For tests that assert browsing is (or
    /// is not) reaching the network.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_browsing(&self) -> bool {
        match self {
            Self::Builtin(builtin) => {
                builtin.cast.is_browsing()
                    || builtin.dlna.is_browsing()
                    || builtin.airplay.is_browsing()
            }
            Self::Reported(reported) => reported.reported.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{RendererConnection, RendererKind};

    fn cast_service(instance: &str, id: &str, addr: &str) -> ReportedRenderer {
        ReportedRenderer {
            service_type: RendererServiceType::GoogleCast,
            instance_name: instance.to_string(),
            addr: addr.to_string(),
            port: 8009,
            txt: [("id", id), ("fn", instance)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn airplay_service(instance: &str, device_id: &str) -> ReportedRenderer {
        ReportedRenderer {
            service_type: RendererServiceType::AirPlay,
            instance_name: instance.to_string(),
            addr: "10.0.0.8".to_string(),
            port: 7000,
            txt: [("deviceid", device_id), ("features", "0x0,0x10000")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    /// A reported service becomes a picker device, and is gone once the host says
    /// its browser lost it.
    #[test]
    fn reported_services_appear_and_disappear() {
        let mut discovery = RendererDiscovery::reported();
        discovery.start();

        discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
        discovery.report(airplay_service("Studio", "AA:BB:CC:DD:EE:FF"));
        let devices = discovery.devices();
        assert_eq!(
            devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["Kitchen", "Studio"],
            "both reported services are in the picker list, sorted by name"
        );
        assert_eq!(devices[0].kind(), RendererKind::Cast);
        assert_eq!(devices[1].kind(), RendererKind::AirPlay);
        assert!(matches!(
            devices[0].connection,
            RendererConnection::Cast { port: 8009, .. }
        ));

        discovery.forget(RendererServiceType::GoogleCast, "Kitchen");
        assert_eq!(
            discovery
                .devices()
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Studio"],
            "a lost service leaves the list"
        );
    }

    /// Reports outside a browse are dropped, and starting a browse clears what
    /// the previous one found — the picker never opens on a stale list.
    #[test]
    fn reports_need_a_live_browse() {
        let mut discovery = RendererDiscovery::reported();

        discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
        assert!(
            discovery.devices().is_empty(),
            "a report outside a browse is dropped"
        );

        discovery.start();
        discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
        assert_eq!(discovery.devices().len(), 1);

        discovery.stop();
        discovery.start();
        assert!(
            discovery.devices().is_empty(),
            "a fresh browse starts from nothing"
        );
    }

    /// A service that can't be reached or routed to is dropped rather than
    /// listed: an unparseable address, or a Cast service with no device id.
    #[test]
    fn unusable_services_are_dropped() {
        let mut discovery = RendererDiscovery::reported();
        discovery.start();

        discovery.report(cast_service("Bad Address", "cast-2", "not-an-address"));
        let mut no_id = cast_service("No Id", "", "192.168.1.9");
        no_id.txt.remove("id");
        discovery.report(no_id);

        assert!(discovery.devices().is_empty());
    }
}
