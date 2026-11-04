//! AirPlay 2: the HomeKit pair-verify that follows transient pair-setup, and the
//! ChaCha20-Poly1305 encryption keyed from it.
//!
//! After the SRP transient pair-setup ([`super::pairing`]) establishes a session,
//! an AirPlay 2 sender runs **pair-verify**: an ephemeral X25519 exchange, each
//! side signing the exchanged keys with an Ed25519 identity, wrapped in
//! ChaCha20-Poly1305 with HKDF-SHA512-derived keys (openairplay spec §9;
//! semantics from the MIT `airplay2-rs` and pyatv). The X25519 shared secret it
//! yields keys the audio-packet cipher: each realtime audio packet is
//! ChaCha20-Poly1305 sealed with an 8-byte counter nonce, the RTP timestamp+SSRC
//! as associated data, and the nonce appended to the datagram — the AirPlay 2
//! realtime convention pyatv streams to HomePods with.
//!
//! Sources: pyatv (MIT, `master` @ 2024) and `jburnhams/airplay2-rs` (MIT, `main`
//! @ 2026). Crates: x25519-dalek 2, ed25519-dalek 2, chacha20poly1305 0.10,
//! hkdf 0.12.
//!
//! This is the transport-agnostic crypto: it produces and consumes each message
//! *body*, so the session drives the POSTs and the tests drive it against a
//! scripted fake receiver that runs the verify server independently.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signer, SigningKey};
use x25519_dalek::{PublicKey, StaticSecret};

use super::ap2_channel::hkdf32;

use super::pairing::PairingError;
use super::secure_rng::SecureRng;
use super::tlv8::{state, tlv_type, Tlv8};
use super::{check_no_error, expect_state};

/// The nonce prefix for the pair-verify messages: four zero bytes then the label.
pub(super) fn pair_verify_nonce(label: &[u8; 8]) -> Nonce {
    super::ap2_channel::nonce_with_tail(label)
}

/// The pair-verify state machine, driven M1→M3 and yielding the X25519 shared
/// secret once M4 arrives. The sender's Ed25519 identity is ephemeral: transient
/// sessions store nothing.
pub(crate) struct PairVerify {
    phase: PairVerifyPhase,
    ephemeral: StaticSecret,
    ephemeral_public: PublicKey,
    signing: SigningKey,
}

enum PairVerifyPhase {
    Init,
    /// M1 sent; awaiting the receiver's M2 (its ephemeral key + signed identity).
    AwaitingM2,
    /// M3 sent; the shared secret is established.
    AwaitingM4([u8; 32]),
    Done,
}

impl PairVerify {
    pub(crate) fn new() -> Self {
        let ephemeral = StaticSecret::random_from_rng(SecureRng);
        let ephemeral_public = PublicKey::from(&ephemeral);
        PairVerify {
            phase: PairVerifyPhase::Init,
            ephemeral,
            ephemeral_public,
            signing: SigningKey::generate(&mut SecureRng),
        }
    }

    /// The M1 body: state M1 and our ephemeral X25519 public key.
    pub(crate) fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, PairVerifyPhase::Init) {
            return Err(PairingError::WrongState);
        }
        self.phase = PairVerifyPhase::AwaitingM2;
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M1)
            .push(tlv_type::PUBLIC_KEY, self.ephemeral_public.as_bytes().to_vec())
            .encode())
    }

    /// Handle the receiver's M2 (its ephemeral key + encrypted signed identity):
    /// derive the shared secret and the session key, decrypt M2 to authenticate
    /// the channel, and return the M3 body carrying our own signed identity.
    ///
    /// The receiver's signature is not checked against a stored identity —
    /// transient pairing is unauthenticated, and the sender holds no device
    /// long-term key. Decryption's AEAD tag still authenticates that both sides
    /// derived the same key.
    pub(crate) fn handle_m2(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, PairVerifyPhase::AwaitingM2) {
            return Err(PairingError::WrongState);
        }
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M2)?;

        let device_public = tlv
            .get(tlv_type::PUBLIC_KEY)
            .ok_or(PairingError::MissingField("device ephemeral key"))?;
        #[expect(
            clippy::map_err_ignore,
            reason = "a slice of the wrong length says nothing past its length"
        )]
        let device_public: [u8; 32] = device_public
            .try_into()
            .map_err(|_| PairingError::MissingField("device ephemeral key length"))?;
        let encrypted = tlv
            .get(tlv_type::ENCRYPTED_DATA)
            .ok_or(PairingError::MissingField("encrypted verifier"))?;

        let shared = self.ephemeral.diffie_hellman(&PublicKey::from(device_public)).to_bytes();
        let session_key = hkdf32(b"Pair-Verify-Encrypt-Salt", &shared, b"Pair-Verify-Encrypt-Info");
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));

        // Decrypt M2's verifier: authenticates the channel (a wrong session key
        // fails the AEAD tag). Its contents aren't checked further in a transient
        // session.
        #[expect(
            clippy::map_err_ignore,
            reason = "an AEAD failure is opaque by design: it says only that the tag did not verify"
        )]
        cipher
            .decrypt(&pair_verify_nonce(b"PV-Msg02"), encrypted)
            .map_err(|_| PairingError::Rejected(0x02))?;

        // Sign our_ephemeral || device_ephemeral and seal it as M3.
        let mut signed = Vec::with_capacity(64);
        signed.extend_from_slice(self.ephemeral_public.as_bytes());
        signed.extend_from_slice(&device_public);
        let signature = self.signing.sign(&signed);

        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, b"bae".to_vec())
            .push(tlv_type::SIGNATURE, signature.to_bytes().to_vec())
            .encode();
        #[expect(
            clippy::map_err_ignore,
            reason = "an AEAD failure is opaque by design: it says only that the tag did not verify"
        )]
        let sealed = cipher
            .encrypt(&pair_verify_nonce(b"PV-Msg03"), Payload { msg: &inner, aad: &[] })
            .map_err(|_| PairingError::Rejected(0x02))?;

        self.phase = PairVerifyPhase::AwaitingM4(shared);
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M3)
            .push(tlv_type::ENCRYPTED_DATA, sealed)
            .encode())
    }

    /// Handle the receiver's M4: on success, the pair-verify shared secret the
    /// audio cipher keys from.
    pub(crate) fn handle_m4(&mut self, response_body: &[u8]) -> Result<[u8; 32], PairingError> {
        let PairVerifyPhase::AwaitingM4(shared) =
            std::mem::replace(&mut self.phase, PairVerifyPhase::Done)
        else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M4)?;
        Ok(shared)
    }
}

/// The ChaCha20-Poly1305 cipher an AirPlay 2 sender applies to each audio packet.
/// Keyed from the pair-verify shared secret; the 8-byte counter increments per
/// packet and rides at the tail of the datagram.
pub(crate) struct Ap2AudioCipher {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl Ap2AudioCipher {
    /// Derive the audio key (`shk`) from the pair-verify shared secret — the same
    /// key the SETUP streams plist hands the receiver.
    pub(crate) fn from_shared_secret(shared: &[u8; 32]) -> Self {
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
