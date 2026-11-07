//! The shape every protocol discovery shares: a published device list, and a
//! browse of the network that is live only while something lists receivers.
//!
//! Cast, UPnP, and AirPlay differ entirely in what they hold while browsing —
//! an mDNS daemon, an SSDP socket and stop flag, one reader thread or two — and
//! not at all in how a caller drives them. [`Browse`] is the part that differs;
//! [`ProtocolDiscovery`] is the part that does not.

use std::net::IpAddr;

use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::RendererDevice;
use bae_diagnostics::Diagnostics;

/// The address to reach a service on: the first IPv4 one, else the first of
/// any family. Receivers advertise IPv4, and the LAN control and media
/// connections are IPv4.
pub(crate) fn preferred_address(addresses: impl Iterator<Item = IpAddr>) -> Option<IpAddr> {
    let mut chosen: Option<IpAddr> = None;
    for addr in addresses {
        if addr.is_ipv4() {
            return Some(addr);
        }
        chosen.get_or_insert(addr);
    }
    chosen
}

/// The instance label of a service fullname (`"Living Room._airplay._tcp.local."`
/// → `"Living Room"`): what precedes the first of `service_markers` the
/// fullname holds, else the whole fullname.
pub(crate) fn instance_label<'a>(fullname: &'a str, service_markers: &[&str]) -> &'a str {
    service_markers
        .iter()
        .find_map(|marker| fullname.split_once(marker))
        .map_or(fullname, |(instance, _)| instance)
}

/// One protocol's live browse: everything it holds while it is reading the
/// network. Started against the list it publishes into, and stopped as a whole.
pub(crate) trait Browse: Sized {
    /// Start reading the network, publishing what is found into `devices`.
    /// `None` when the transport could not open what it browses with — it has
    /// logged why and released whatever it had taken. A reader that panics on
    /// what it read reports it through `diagnostics`, skips it, and reads on.
    fn start(devices: &PublishedDevices, diagnostics: &Diagnostics) -> Option<Self>;

    /// Stop reading and release everything the browse holds. The published list
    /// is left alone, so the last devices found stay on screen.
    fn stop(self);
}

/// One protocol's discovery: the [`RendererDevice`] list it publishes, plus its
/// browse while one is running. The picker starts every protocol when it opens
/// and stops them when it closes; a stopped discovery holds no daemon, socket,
/// or thread.
pub(crate) struct ProtocolDiscovery<B: Browse> {
    devices: PublishedDevices,
    /// The running browse, `None` while stopped.
    running: Option<B>,
}

impl<B: Browse> ProtocolDiscovery<B> {
    /// A discovery that is not browsing and lists no device.
    pub(crate) fn new() -> Self {
        Self { devices: PublishedDevices::new(), running: None }
    }

    /// Follow the live device list.
    pub(crate) fn subscribe(&self) -> crate::renderer::DeviceListWatch {
        self.devices.subscribe()
    }

    /// The current device list snapshot.
    pub(crate) fn devices(&self) -> Vec<RendererDevice> {
        self.devices.current()
    }

    /// Begin browsing. Idempotent: a second call while already browsing is a
    /// no-op, and a transport that cannot start stays stopped.
    pub(crate) fn start(&mut self, diagnostics: &Diagnostics) {
        if self.running.is_some() {
            return;
        }
        self.running = B::start(&self.devices, diagnostics);
    }

    /// Stop browsing and release what the browse held. The last published
    /// device list is kept.
    pub(crate) fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop();
        }
    }
}

impl<B: Browse> Drop for ProtocolDiscovery<B> {
    fn drop(&mut self) {
        self.stop();
    }
}
