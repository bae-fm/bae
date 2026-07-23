//! Google Cast: the CASTV2 wire of a remote renderer.
//!
//! bae plays to Chromecast/Nest devices as one flavor of remote renderer (see
//! [`crate::renderer`]). This module holds only what's Cast-specific: mDNS
//! discovery of devices ([`discovery`]) and the CASTV2 control channel
//! ([`channel::RustCastChannel`]) that implements the shared
//! [`crate::renderer::RendererChannel`]. The session, media, status, and format
//! types it drives are the shared renderer ones — the receiver fetches audio over
//! HTTP itself, exactly like a UPnP renderer.

pub mod channel;
pub mod discovery;

pub use channel::RustCastChannel;
pub use discovery::CastDiscovery;

#[cfg(test)]
mod tests;
