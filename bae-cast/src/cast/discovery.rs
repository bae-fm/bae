//! mDNS discovery of Cast devices on the local network.
//!
//! `CastDiscovery` browses `_googlecast._tcp.local.` and keeps a live list of
//! reachable devices in a `PublishedDevices`. Browsing is not always-on: the
//! cast controller starts it when a place that lists receivers opens and stops
//! it when the last one closes.

use std::collections::HashMap;
use std::net::IpAddr;

use bae_diagnostics::{Diagnostics, ReportedThread};
use bae_util::task_failure::caught_sync;
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tracing::{debug, warn};

use crate::renderer::discovery::RendererServiceType;
use crate::renderer::protocol_discovery::{
    instance_label, preferred_address, stated, Browse, ProtocolDiscovery,
};
use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::{RendererConnection, RendererDevice};

/// Browses for Cast devices and publishes the current list as
/// [`RendererDevice`]s so the picker shows one merged list. Start and stop
/// with the picker's visibility; a stopped discovery holds no mDNS daemon and no
/// browse thread.
pub(crate) type CastDiscovery = ProtocolDiscovery<CastBrowse>;

/// A live Cast browse: the mDNS daemon and the thread draining its events.
pub(crate) struct CastBrowse {
    daemon: ServiceDaemon,
    reader: ReportedThread,
}

impl Browse for CastBrowse {
    fn start(devices: &PublishedDevices, diagnostics: &Diagnostics) -> Option<Self> {
        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                warn!("cast discovery: failed to start mDNS daemon: {e}");
                return None;
            }
        };
        let service_type = RendererServiceType::GoogleCast.mdns_service_type();
        let events = match daemon.browse(service_type) {
            Ok(events) => events,
            Err(e) => {
                warn!("cast discovery: failed to browse {service_type}: {e}");
                if let Err(shutdown_err) = daemon.shutdown() {
                    debug!(
                        "cast discovery: mDNS daemon shutdown after browse failure: {shutdown_err}"
                    );
                }
                return None;
            }
        };
        devices.clear();
        let reader_devices = devices.clone();
        let reader_diagnostics = diagnostics.clone();
        // A reader that panicked can no longer keep the list current, so the
        // list is emptied rather than left showing receivers it can't confirm.
        let panicked_devices = devices.clone();
        let reader = match diagnostics.spawn_thread_reported(
            "cast browse reader",
            move || run_browse(events, reader_devices, reader_diagnostics),
            move |_| panicked_devices.clear(),
        ) {
            Ok(reader) => reader,
            Err(error) => {
                warn!("cast discovery: failed to start the browse reader thread: {error}");
                if let Err(shutdown_err) = daemon.shutdown() {
                    debug!(
                        "cast discovery: mDNS daemon shutdown after reader failure: {shutdown_err}"
                    );
                }
                return None;
            }
        };
        Some(Self { daemon, reader })
    }

    fn stop(self) {
        // Shutting the daemon down closes the event channel, so the reader
        // thread's loop ends and it can be joined.
        if let Err(e) = self.daemon.shutdown() {
            warn!("cast discovery: mDNS daemon shutdown failed: {e}");
        }
        self.reader.join();
    }
}

/// Drain the mDNS event channel, maintaining the device list keyed by service
/// fullname, and publish a de-duplicated snapshot on every change. Returns when
/// the channel closes (the daemon shut down). An event whose handling panics
/// is reported and skipped; the browse reads on.
fn run_browse(
    events: mdns_sd::Receiver<ServiceEvent>,
    devices: PublishedDevices,
    diagnostics: Diagnostics,
) {
    // Keyed by service fullname (what `ServiceRemoved` carries), so a device can
    // be dropped when it leaves.
    let mut by_fullname: HashMap<String, RendererDevice> = HashMap::new();
    while let Ok(event) = events.recv() {
        let followed = caught_sync("follow a Cast advertisement", || {
            if apply_event(&mut by_fullname, event) {
                devices.publish(snapshot(&by_fullname));
            }
        });
        if let Err(bug) = followed {
            diagnostics.panicked(bug);
        }
    }
}

/// Apply one mDNS event to the device table: whether it changed.
fn apply_event(by_fullname: &mut HashMap<String, RendererDevice>, event: ServiceEvent) -> bool {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let fullname = resolved.fullname.clone();
            match device_from_resolved(&resolved) {
                Some(device) => {
                    by_fullname.insert(fullname, device);
                    true
                }
                None => {
                    debug!("cast discovery: ignoring unusable service {fullname}");
                    false
                }
            }
        }
        ServiceEvent::ServiceRemoved(_ty, fullname) => by_fullname.remove(&fullname).is_some(),
        // Search lifecycle and pre-resolution "found" events carry no
        // address, and `ServiceEvent` is non-exhaustive, so anything else is
        // ignored until it's handled above.
        _ => false,
    }
}

/// A stable, de-duplicated device list: one entry per device id (a device seen
/// on several addresses collapses to one), sorted by name for a stable UI.
fn snapshot(by_fullname: &HashMap<String, RendererDevice>) -> Vec<RendererDevice> {
    let mut devices = dedupe_by_id(by_fullname.values());
    crate::renderer::discovery::sort_for_picker(&mut devices);
    devices
}

/// One entry per device id: the same Cast device seen on several addresses or
/// service instances collapses to one. Shared with the reported-discovery path,
/// which merges services the host's browser found the same way.
pub(crate) fn dedupe_by_id<'a>(
    devices: impl Iterator<Item = &'a RendererDevice>,
) -> Vec<RendererDevice> {
    let mut by_id: HashMap<&str, RendererDevice> = HashMap::new();
    for device in devices {
        by_id.entry(&device.id).or_insert_with(|| device.clone());
    }
    by_id.into_values().collect()
}

/// Map a resolved mDNS service to a Cast [`RendererDevice`]. The `ResolvedService` type
/// is opaque (non-exhaustive), so the pieces are pulled out here and mapped by
/// the pure [`map_device`], which the tests drive directly.
fn device_from_resolved(resolved: &ResolvedService) -> Option<RendererDevice> {
    map_device(
        instance_name(&resolved.fullname),
        resolved.port,
        resolved.addresses.iter().map(|scoped| scoped.to_ip_addr()),
        resolved.txt_properties.get_property_val_str("id"),
        resolved.txt_properties.get_property_val_str("fn"),
        resolved.txt_properties.get_property_val_str("md"),
    )
}

/// Build a Cast [`RendererDevice`] from a service's fields, or `None` if it
/// lacks what's needed to reach and name it: a device id, and at least one
/// address. IPv4 is preferred — Cast receivers advertise it and the media
/// receiver fetches over IPv4 LAN. The name falls back to the service instance
/// label when a device advertises no `fn` value. Its model is the TXT `md`
/// record, when it states one.
pub(crate) fn map_device(
    instance: &str,
    port: u16,
    addresses: impl Iterator<Item = IpAddr>,
    id: Option<&str>,
    friendly_name: Option<&str>,
    model: Option<&str>,
) -> Option<RendererDevice> {
    let id = id.map(str::trim).filter(|id| !id.is_empty())?;
    let name =
        friendly_name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(instance).to_string();

    Some(RendererDevice {
        id: id.to_string(),
        name,
        model: stated(model),
        connection: RendererConnection::Cast { addr: preferred_address(addresses)?, port },
    })
}

/// The instance label of a service fullname (`"Living Room._googlecast._tcp.local."`
/// → `"Living Room"`), which is what a host browser reports directly.
pub(super) fn instance_name(fullname: &str) -> &str {
    instance_label(fullname, &["._googlecast"])
}
