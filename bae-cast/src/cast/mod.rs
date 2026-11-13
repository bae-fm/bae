//! Google Cast: the CASTV2 wire of a remote renderer.
//!
//! bae plays to Chromecast/Nest devices as one flavor of remote renderer (see
//! [`bae_playback::renderer`]). This module holds only what's Cast-specific: mDNS
//! discovery of devices ([`discovery`]) and the CASTV2 control channel
//! ([`channel::CastChannel`], over bae's own Cast client in [`client`]) that implements the shared
//! [`bae_playback::renderer::RendererChannel`]. The session, media, status, and format
//! types it drives are the shared renderer ones — the receiver fetches audio over
//! HTTP itself, exactly like a UPnP renderer.

pub(crate) mod channel;
mod client;
mod connection;
pub(crate) mod discovery;
#[cfg(test)]
pub(crate) mod fake_receiver;
mod messages;
mod wire;

pub(crate) use channel::CastChannel;
pub(crate) use client::CastClient;
pub(crate) use discovery::CastDiscovery;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
