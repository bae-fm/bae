//! Remote renderers as the device picker sees them: one merged list of the
//! Cast, UPnP and AirPlay devices on the network, and where it comes from.
//!
//! Each protocol module (`crate::cast`, `crate::dlna`, `crate::airplay`)
//! maps what its devices advertise to a [`RendererDevice`]; [`discovery`] runs
//! their browses side by side, or takes the services a host's browser reports,
//! and merges them into the one [`device`] list. A device carries the
//! flavor-specific [`RendererConnection`] the cast controller connects to.

pub mod device;
pub mod discovery;
pub(crate) mod protocol_discovery;
pub(crate) mod published_devices;

pub use device::{RendererConnection, RendererDevice, RendererKind};
pub use discovery::{
    RendererDiscovery, RendererServiceType, ReportedRenderer, RENDERER_SERVICE_TYPES,
};
