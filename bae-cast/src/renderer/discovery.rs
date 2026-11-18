//! Where the device picker's list comes from.
//!
//! bae browses the network itself ([`BuiltinDiscovery`]): mDNS for Cast and
//! AirPlay, SSDP for UPnP, and it merges the three lists into one
//! [`RendererDevice`] list, started and stopped with the picker's visibility.
//! Tests run the cast controller over [`reported::ReportedDiscovery`] instead,
//! which takes services a test hands it and holds no socket; it maps each one
//! through the same code bae's own browse uses (`crate::cast::discovery`,
//! `crate::airplay::discovery`).

use crate::airplay::AirPlayDiscovery;
use crate::cast::CastDiscovery;
use crate::dlna::DlnaDiscovery;
use crate::renderer::RendererDevice;

/// A DNS-SD service type a renderer advertises itself on, which decides how
/// bae reads its TXT record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererServiceType {
    /// Google Cast receivers.
    GoogleCast,
    /// AirPlay 2 and newer AirPlay 1 receivers.
    AirPlay,
    /// Legacy RAOP (AirPlay 1) receivers.
    Raop,
}

impl RendererServiceType {
    /// The fully-qualified type bae's own mDNS browse subscribes to.
    pub(crate) fn mdns_service_type(self) -> &'static str {
        match self {
            Self::GoogleCast => "_googlecast._tcp.local.",
            Self::AirPlay => "_airplay._tcp.local.",
            Self::Raop => "_raop._tcp.local.",
        }
    }
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
    fn new() -> Self {
        Self {
            cast: CastDiscovery::new(),
            dlna: DlnaDiscovery::new(),
            airplay: AirPlayDiscovery::new(),
        }
    }
}

/// One entry per device, sorted by name with the id breaking ties, so the picker
/// order is stable across requeries.
pub(crate) fn sort_for_picker(devices: &mut [RendererDevice]) {
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
}

/// The picker's device source: bae's own browse, or in tests the services a
/// test reports.
pub enum RendererDiscovery {
    Builtin(BuiltinDiscovery),
    #[cfg(test)]
    Reported(reported::ReportedDiscovery),
}

impl RendererDiscovery {
    /// bae browses the network itself.
    pub fn builtin() -> Self {
        Self::Builtin(BuiltinDiscovery::new())
    }

    /// A test's device source, which lists the services the test reports.
    #[cfg(test)]
    pub fn reported() -> Self {
        Self::Reported(reported::ReportedDiscovery::new())
    }

    /// Every list to watch for changes — one per underlying browse — so a caller
    /// can invalidate an open picker as devices come and go.
    pub fn subscribe(&self) -> Vec<crate::renderer::DeviceListWatch> {
        match self {
            Self::Builtin(builtin) => vec![
                builtin.cast.subscribe(),
                builtin.dlna.subscribe(),
                builtin.airplay.subscribe(),
            ],
            #[cfg(test)]
            Self::Reported(reported) => vec![reported.subscribe()],
        }
    }

    /// Begin browsing. Idempotent. bae's own readers run under
    /// `diagnostics`, so one that panics ends.
    pub fn start(&mut self, diagnostics: &bae_diagnostics::Diagnostics) {
        match self {
            Self::Builtin(builtin) => {
                builtin.cast.start(diagnostics);
                builtin.dlna.start(diagnostics);
                builtin.airplay.start(diagnostics);
            }
            #[cfg(test)]
            Self::Reported(reported) => reported.start(),
        }
    }

    /// Stop browsing. The last published list is kept, as it
    /// is on the builtin path.
    pub fn stop(&mut self) {
        match self {
            Self::Builtin(builtin) => {
                builtin.cast.stop();
                builtin.dlna.stop();
                builtin.airplay.stop();
            }
            #[cfg(test)]
            Self::Reported(reported) => reported.stop(),
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
            #[cfg(test)]
            Self::Reported(reported) => reported.devices(),
        }
    }

    /// Take a service a test reports. bae's own browse reads the network
    /// itself and takes no report.
    #[cfg(test)]
    pub fn report(&mut self, service: reported::ReportedRenderer) {
        match self {
            Self::Builtin(_) => panic!("bae's own browse takes no reported service"),
            Self::Reported(reported) => reported.report(service),
        }
    }

    /// Drop a service a test reported.
    #[cfg(test)]
    pub fn forget(&mut self, service_type: RendererServiceType, instance_name: &str) {
        match self {
            Self::Builtin(_) => panic!("bae's own browse takes no reported service"),
            Self::Reported(reported) => reported.forget(service_type, instance_name),
        }
    }
}

#[cfg(test)]
pub(crate) mod reported;

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
