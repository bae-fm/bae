//! A discovered remote renderer, merged across flavors.
//!
//! Cast discovery (`crate::cast`) and UPnP discovery (`crate::dlna`) both
//! produce [`RendererDevice`]s so the picker shows one list — a speaker is a
//! speaker, not segregated by protocol. Each device carries the flavor-specific
//! [`RendererConnection`] needed to build its channel, and reports its
//! [`RendererKind`] for the UI.

use std::net::IpAddr;

use bae_playback::renderer::{CastReceiver, RendererKind};

use crate::airplay::AirPlayCapabilities;

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
    /// The model the device states of itself; `None` when it states none.
    pub model: Option<String>,
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

    /// The receiver this device is, as the picker lists it and playback
    /// names the one it plays on.
    pub fn receiver(&self) -> CastReceiver {
        CastReceiver {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind(),
            model: self.model.clone(),
            address: self.connection.address(),
        }
    }
}

/// How to reach a device's control channel — the flavor-specific coordinates the
/// cast controller uses to build the right [`bae_playback::renderer::RendererChannel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererConnection {
    /// A Cast receiver at a LAN address; the CASTV2 channel connects to it.
    Cast { addr: IpAddr, port: u16 },
    /// A UPnP MediaRenderer, addressed by its resolved SOAP control URLs.
    Dlna {
        /// The host the device's description was fetched from.
        host: String,
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
        capabilities: Result<AirPlayCapabilities, crate::airplay::UnreadableSetting>,
    },
}

impl RendererConnection {
    /// The host discovery found the device at, as text.
    pub fn address(&self) -> String {
        match self {
            Self::Cast { addr, .. } | Self::AirPlay { addr, .. } => addr.to_string(),
            Self::Dlna { host, .. } => host.clone(),
        }
    }
}
