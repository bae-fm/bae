//! AirPlay: bae as an AirPlay sender.
//!
//! bae plays to AirPlay receivers — HomePods, Apple TVs, AirPlay-2 speakers, and
//! legacy AirPlay 1 / RAOP gear. This is the third renderer flavor and the first
//! push-audio one: unlike a Cast or UPnP receiver, an AirPlay receiver does *not*
//! fetch a URL. bae keeps decoding locally and pushes timed, encrypted audio
//! packets, so AirPlay is deliberately its own renderer variant rather than a
//! [`bae_core::renderer::RendererChannel`] (whose contract is "the device fetches a
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
//! announce, [`rtsp`] is the control connection both dialects drive, and
//! [`sink`] opens either dialect's session behind bae-core's AirPlay output.

pub(crate) mod airplay2;
pub(crate) mod ap2_channel;
pub(crate) mod ap2_session;
pub(crate) mod bplist;
pub(crate) mod capabilities;
pub(crate) mod crypto;
pub(crate) mod discovery;
pub(crate) mod pairing;
pub(crate) mod rtp;
pub(crate) mod rtsp;
mod secure_rng;
pub(crate) mod session;
pub(crate) mod sink;
pub(crate) mod srp;
pub(crate) mod stream;
pub(crate) mod tlv8;

use pairing::PairingError;
use tlv8::{tlv_type, Tlv8};

pub(crate) use capabilities::{AirPlayCapabilities, UnreadableSetting};
pub(crate) use discovery::AirPlayDiscovery;

/// The receiver's Error TLV, if it sent one: pairing stops on any nonzero
/// code. Both dialects' pairing steps check this before reading anything else.
fn check_no_error(tlv: &Tlv8) -> Result<(), PairingError> {
    match tlv.get_u8(tlv_type::ERROR) {
        Some(code) if code != 0 => Err(PairingError::Rejected(code)),
        _ => Ok(()),
    }
}

/// The step the receiver's response has to be answering.
fn expect_state(tlv: &Tlv8, expected: u8) -> Result<(), PairingError> {
    let actual = tlv.get_u8(tlv_type::STATE);
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(PairingError::UnexpectedState { expected, actual })
    }
}
