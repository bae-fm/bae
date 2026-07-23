//! A discovered remote renderer, merged across flavors.
//!
//! Cast discovery ([`crate::cast`]) and UPnP discovery ([`crate::dlna`]) both
//! produce [`RendererDevice`]s so the picker shows one list — a speaker is a
//! speaker, not segregated by protocol. Each device carries the flavor-specific
//! [`RendererConnection`] needed to build its channel, and reports its
//! [`RendererKind`] for the UI.

use std::net::IpAddr;

use crate::airplay::AirPlayCapabilities;

/// Which flavor of remote renderer a device is. Surfaced to the UI as a tag; the
/// picker does not segregate the list by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    Cast,
    Dlna,
    /// An AirPlay receiver (RAOP or AirPlay 2). Unlike Cast/DLNA it does not fetch
    /// a URL — bae pushes decoded audio to it — but it shares the one device list.
    AirPlay,
}

/// A discovered remote renderer: enough to display it and to connect a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererDevice {
    /// Stable device identifier, used to route playback and de-duplicate a
    /// device seen more than once. The id schemes don't overlap across flavors (a
    /// Cast device id vs. a UPnP `uuid:…` UDN), so ids are unique in the merged
    /// list without a flavor prefix; the flavor itself rides in `connection`.
    pub id: String,
    /// Human-readable name to show in the picker.
    pub name: String,
    /// The flavor-specific address the channel connects to.
    pub connection: RendererConnection,
}

impl RendererDevice {
    /// The device's flavor, derived from its connection.
    pub fn kind(&self) -> RendererKind {
        match self.connection {
            RendererConnection::Cast { .. } => RendererKind::Cast,
            RendererConnection::Dlna { .. } => RendererKind::Dlna,
            RendererConnection::AirPlay { .. } => RendererKind::AirPlay,
        }
    }
}

/// How to reach a device's control channel — the flavor-specific coordinates the
/// desktop side uses to build the right [`crate::renderer::RendererChannel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererConnection {
    /// A Cast receiver at a LAN address; the CASTV2 channel connects to it.
    Cast { addr: IpAddr, port: u16 },
    /// A UPnP MediaRenderer, addressed by its resolved SOAP control URLs.
    Dlna {
        av_transport_url: String,
        /// `None` when the renderer advertises no RenderingControl service
        /// (volume is then unsupported).
        rendering_control_url: Option<String>,
    },
    /// An AirPlay receiver at a LAN address, with what it announced about itself
    /// (the dialect, whether it needs a PIN, the RAOP audio parameters) so the
    /// sender can decide how to connect.
    AirPlay {
        addr: IpAddr,
        port: u16,
        capabilities: AirPlayCapabilities,
    },
}
