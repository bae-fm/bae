//! The device source the cast controller's tests run over: it lists the
//! services a test reports, as a host's own service browser would find them,
//! and holds no socket. Each service maps to a device through the code bae's
//! own browse uses.

use std::collections::HashMap;
use std::net::IpAddr;

use tracing::debug;

use super::{sort_for_picker, RendererServiceType};
use crate::airplay::discovery::{AirPlayDevice, AirPlayService};
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::RendererDevice;

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
        Self { devices: PublishedDevices::new(), reported: None }
    }

    /// Follow the live device list.
    pub fn subscribe(&self) -> crate::renderer::DeviceListWatch {
        self.devices.subscribe()
    }

    /// The current device list snapshot.
    pub fn devices(&self) -> Vec<RendererDevice> {
        self.devices.current()
    }

    /// Whether a browse is live right now.
    pub fn is_browsing(&self) -> bool {
        self.reported.is_some()
    }

    /// Begin browsing. Idempotent: a second call while already browsing is a
    /// no-op. A fresh browse starts from nothing, so nothing opens on what a
    /// previous one found.
    pub fn start(&mut self) {
        if self.reported.is_some() {
            return;
        }
        self.reported = Some(HashMap::new());
        self.devices.clear();
    }

    /// Stop browsing: later reports accumulate nothing. The last published
    /// device list is kept.
    pub fn stop(&mut self) {
        self.reported = None;
    }

    /// Take a service the host resolved. Ignored when the host reports outside a
    /// browse, and when the service carries too little to reach or name the
    /// device.
    pub fn report(&mut self, service: ReportedRenderer) {
        let Some(reported) = self.reported.as_mut() else {
            debug!("reported discovery: ignoring {} while not browsing", service.instance_name);
            return;
        };
        let key = (service.service_type, service.instance_name.clone());
        let Some(entry) = map_reported(&service) else {
            debug!("reported discovery: ignoring unusable service {}", service.instance_name);
            return;
        };
        reported.insert(key, entry);
        self.devices.publish(snapshot(reported));
    }

    /// Drop a service the host's browser no longer sees.
    pub fn forget(&mut self, service_type: RendererServiceType, instance_name: &str) {
        let Some(reported) = self.reported.as_mut() else {
            return;
        };
        if reported.remove(&(service_type, instance_name.to_string())).is_none() {
            return;
        }
        self.devices.publish(snapshot(reported));
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
    let txt: HashMap<String, String> =
        service.txt.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect();
    match service.service_type {
        RendererServiceType::GoogleCast => crate::cast::discovery::map_device(
            &service.instance_name,
            service.port,
            std::iter::once(addr),
            txt.get("id").map(String::as_str),
            txt.get("fn").map(String::as_str),
            txt.get("md").map(String::as_str),
        )
        .map(ReportedEntry::Cast),
        RendererServiceType::AirPlay => map_airplay(AirPlayService::AirPlay, service, addr, &txt),
        RendererServiceType::Raop => map_airplay(AirPlayService::Raop, service, addr, &txt),
    }
}

/// A reported AirPlay or RAOP service, mapped by the code bae's own browse uses.
fn map_airplay(
    airplay: AirPlayService,
    service: &ReportedRenderer,
    addr: IpAddr,
    txt: &HashMap<String, String>,
) -> Option<ReportedEntry> {
    crate::airplay::discovery::map_device(
        airplay,
        &service.instance_name,
        service.port,
        std::iter::once(addr),
        txt,
    )
    .map(ReportedEntry::AirPlay)
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
