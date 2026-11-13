//! AirPlay 2 audio-packet encryption and the clock choice.
//!
//! The SRP key from transient pair-setup ([`super::pairing`]) keys the control
//! channel ([`super::ap2_channel`]) and, through the same HKDF expansion, the
//! audio-packet cipher: each realtime audio packet is ChaCha20-Poly1305 sealed
//! with an 8-byte counter nonce, the RTP timestamp+SSRC as associated data, and
//! the nonce appended to the datagram — the AirPlay 2 realtime convention pyatv
//! streams to HomePods with.
//!
//! Sources: pyatv (MIT, `master` @ 2024). Crates: chacha20poly1305 0.10, hkdf 0.12.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key};

/// The ChaCha20-Poly1305 cipher an AirPlay 2 sender applies to each audio packet.
/// Keyed from the pair-setup session secret; the 8-byte counter increments per
/// packet and rides at the tail of the datagram.
pub(crate) struct Ap2AudioCipher {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl Ap2AudioCipher {
    /// Derive the audio key (`shk`) from the pair-setup session secret — the same
    /// key the SETUP streams plist hands the receiver.
    pub(crate) fn from_shared_secret(shared: &[u8]) -> Self {
        let key = super::ap2_channel::audio_key(shared);
        Ap2AudioCipher { cipher: ChaCha20Poly1305::new(Key::from_slice(&key)), counter: 0 }
    }

    /// Seal one packet's payload: `aad` is the RTP header's timestamp+SSRC (bytes
    /// 4..12). Returns the ciphertext+tag followed by the 8-byte little-endian
    /// nonce counter, which the sender appends after the RTP payload.
    pub(crate) fn seal(&mut self, aad: &[u8], payload: &[u8]) -> Vec<u8> {
        let tail = self.counter.to_le_bytes();
        let sealed = self
            .cipher
            .encrypt(&super::ap2_channel::nonce_with_tail(&tail), Payload { msg: payload, aad })
            .expect("ChaCha20-Poly1305 seal cannot fail for a valid key");
        self.counter = self.counter.wrapping_add(1);

        let mut out = sealed;
        out.extend_from_slice(&tail);
        out
    }

    /// The counter the next `seal` will use.
    #[cfg(test)]
    pub(crate) fn counter(&self) -> u64 {
        self.counter
    }
}

/// Which clock an AirPlay 2 receiver uses, decided from its features — PTP for
/// receivers that advertise it, else the NTP timing responder RAOP already runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimingProtocol {
    /// The receiver participates in the sender's NTP timing (RAOP-style).
    Ntp,
    /// The receiver requires PTP (IEEE 1588) clock synchronization.
    Ptp,
}

impl TimingProtocol {
    /// Bit 42 of the features value marks a receiver that requires PTP timing.
    const SUPPORTS_PTP: u64 = 1 << 42;

    /// Decide the timing protocol from a receiver's features value.
    pub(crate) fn from_features(features: u64) -> Self {
        if features & Self::SUPPORTS_PTP != 0 {
            TimingProtocol::Ptp
        } else {
            TimingProtocol::Ntp
        }
    }
}

#[cfg(test)]
#[path = "airplay2_tests.rs"]
mod tests;
