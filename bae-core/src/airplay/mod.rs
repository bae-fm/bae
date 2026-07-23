//! AirPlay: bae as an AirPlay sender.
//!
//! bae plays to AirPlay receivers — HomePods, Apple TVs, AirPlay-2 speakers, and
//! legacy AirPlay 1 / RAOP gear. This is the third renderer flavor and the first
//! push-audio one: unlike a Cast or UPnP receiver, an AirPlay receiver does *not*
//! fetch a URL. bae keeps decoding locally and pushes timed, encrypted audio
//! packets, so AirPlay is deliberately its own renderer variant rather than a
//! [`crate::renderer::RendererChannel`] (whose contract is "the device fetches a
//! URL and is driven by transport commands").
//!
//! The sender speaks two dialects, chosen from a receiver's announced features,
//! never probed:
//! - **RAOP** (`_raop._tcp`): RTSP + ALAC/PCM over RTP, RSA-AES or unencrypted.
//! - **AirPlay 2** (`_airplay._tcp` with an AirPlay 2 feature bit): HomeKit
//!   transient pairing and ChaCha20-Poly1305 packet encryption.
//!
//! This module holds the parts common to both and the dialect-specific wire:
//! [`discovery`] browses for receivers, [`capabilities`] parses what they
//! announce, and [`rtsp`] is the control connection both dialects drive.

pub mod airplay2;
pub mod ap2_channel;
pub mod ap2_session;
pub mod bplist;
pub mod capabilities;
pub mod crypto;
pub mod discovery;
pub mod pairing;
pub mod rtp;
pub mod rtsp;
mod secure_rng;
pub mod session;
pub mod srp;
pub mod stream;
pub mod tlv8;

pub use capabilities::{AirPlayCapabilities, Dialect, RaopCodec, RaopEncryption, RaopParams};
pub use discovery::{AirPlayDevice, AirPlayDiscovery};
