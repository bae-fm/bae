//! The shape every protocol discovery shares: a published device list, and a
//! browse of the network that is live only while the picker is open.
//!
//! Cast, UPnP, and AirPlay differ entirely in what they hold while browsing —
//! an mDNS daemon, an SSDP socket and stop flag, one reader thread or two — and
//! not at all in how a caller drives them. [`Browse`] is the part that differs;
//! [`ProtocolDiscovery`] is the part that does not.

use crate::renderer::published_devices::PublishedDevices;
use crate::renderer::RendererDevice;

/// One protocol's live browse: everything it holds while it is reading the
/// network. Started against the list it publishes into, and stopped as a whole.
pub(crate) trait Browse: Sized {
    /// Start reading the network, publishing what is found into `devices`.
    /// `None` when the transport could not open what it browses with — it has
    /// logged why and released whatever it had taken.
    fn start(devices: &PublishedDevices) -> Option<Self>;

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
    /// Subscribe to the live device list. The current snapshot is available
    /// immediately on the returned receiver.
    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<Vec<RendererDevice>> {
        self.devices.subscribe()
    }

    /// The current device list snapshot.
    pub(crate) fn devices(&self) -> Vec<RendererDevice> {
        self.devices.current()
    }

    /// Whether this protocol is reaching the network right now. For tests that
    /// assert browsing is (or is not) live.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn is_browsing(&self) -> bool {
        self.running.is_some()
    }

    /// Begin browsing. Idempotent: a second call while already browsing is a
    /// no-op, and a transport that cannot start stays stopped.
    pub(crate) fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        self.running = B::start(&self.devices);
    }

    /// Stop browsing and release what the browse held. The last published
    /// device list is kept.
    pub(crate) fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop();
        }
    }
}

impl<B: Browse> Default for ProtocolDiscovery<B> {
    fn default() -> Self {
        Self {
            devices: PublishedDevices::new(),
            running: None,
        }
    }
}

impl<B: Browse> Drop for ProtocolDiscovery<B> {
    fn drop(&mut self) {
        self.stop();
    }
}
