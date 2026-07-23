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

/// The nonce prefix for the pair-verify messages: four zero bytes then the label.
fn pair_verify_nonce(label: &[u8; 8]) -> Nonce {
    let mut bytes = [0u8; 12];
    bytes[4..].copy_from_slice(label);
    Nonce::from(bytes)
}

/// The pair-verify state machine, driven M1→M3 and yielding the X25519 shared
/// secret once M4 arrives. The sender's Ed25519 identity is ephemeral: transient
/// sessions store nothing.
pub struct PairVerify {
    phase: Phase,
    ephemeral: StaticSecret,
    ephemeral_public: PublicKey,
    signing: SigningKey,
}

enum Phase {
    Init,
    /// M1 sent; awaiting the receiver's M2 (its ephemeral key + signed identity).
    AwaitingM2,
    /// M3 sent; the shared secret is established.
    AwaitingM4([u8; 32]),
    Done,
}

impl PairVerify {
    pub fn new() -> Self {
        let ephemeral = StaticSecret::random_from_rng(SecureRng);
        let ephemeral_public = PublicKey::from(&ephemeral);
        PairVerify {
            phase: Phase::Init,
            ephemeral,
            ephemeral_public,
            signing: SigningKey::generate(&mut SecureRng),
        }
    }

    /// The M1 body: state M1 and our ephemeral X25519 public key.
    pub fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, Phase::Init) {
            return Err(PairingError::WrongState);
        }
        self.phase = Phase::AwaitingM2;
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M1)
            .push(
                tlv_type::PUBLIC_KEY,
                self.ephemeral_public.as_bytes().to_vec(),
            )
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
    pub fn handle_m2(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, Phase::AwaitingM2) {
            return Err(PairingError::WrongState);
        }
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M2)?;

        let device_public = tlv
            .get(tlv_type::PUBLIC_KEY)
            .ok_or(PairingError::MissingField("device ephemeral key"))?;
        let device_public: [u8; 32] = device_public
            .try_into()
            .map_err(|_| PairingError::MissingField("device ephemeral key length"))?;
        let encrypted = tlv
            .get(tlv_type::ENCRYPTED_DATA)
            .ok_or(PairingError::MissingField("encrypted verifier"))?;

        let shared = self
            .ephemeral
            .diffie_hellman(&PublicKey::from(device_public))
            .to_bytes();
        let session_key = hkdf32(
            b"Pair-Verify-Encrypt-Salt",
            &shared,
            b"Pair-Verify-Encrypt-Info",
        );
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));

        // Decrypt M2's verifier: authenticates the channel (a wrong session key
        // fails the AEAD tag). Its contents aren't checked further in a transient
        // session.
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
        let sealed = cipher
            .encrypt(
                &pair_verify_nonce(b"PV-Msg03"),
                Payload {
                    msg: &inner,
                    aad: &[],
                },
            )
            .map_err(|_| PairingError::Rejected(0x02))?;

        self.phase = Phase::AwaitingM4(shared);
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M3)
            .push(tlv_type::ENCRYPTED_DATA, sealed)
            .encode())
    }

    /// Handle the receiver's M4: on success, the pair-verify shared secret the
    /// audio cipher keys from.
    pub fn handle_m4(&mut self, response_body: &[u8]) -> Result<[u8; 32], PairingError> {
        let Phase::AwaitingM4(shared) = std::mem::replace(&mut self.phase, Phase::Done) else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M4)?;
        Ok(shared)
    }
}

impl Default for PairVerify {
    fn default() -> Self {
        Self::new()
    }
}

fn check_no_error(tlv: &Tlv8) -> Result<(), PairingError> {
    match tlv.get_u8(tlv_type::ERROR) {
        Some(code) if code != 0 => Err(PairingError::Rejected(code)),
        _ => Ok(()),
    }
}

fn expect_state(tlv: &Tlv8, expected: u8) -> Result<(), PairingError> {
    let actual = tlv.get_u8(tlv_type::STATE);
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(PairingError::UnexpectedState { expected, actual })
    }
}

/// The ChaCha20-Poly1305 cipher an AirPlay 2 sender applies to each audio packet.
/// Keyed from the pair-verify shared secret; the 8-byte counter increments per
/// packet and rides at the tail of the datagram.
pub struct Ap2AudioCipher {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl Ap2AudioCipher {
    /// Derive the audio key (`shk`) from the pair-verify shared secret — the same
    /// key the SETUP streams plist hands the receiver.
    pub fn from_shared_secret(shared: &[u8; 32]) -> Self {
        let key = super::ap2_channel::audio_key(shared);
        Ap2AudioCipher {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&key)),
            counter: 0,
        }
    }

    /// Seal one packet's payload: `aad` is the RTP header's timestamp+SSRC (bytes
    /// 4..12). Returns the ciphertext+tag followed by the 8-byte little-endian
    /// nonce counter, which the sender appends after the RTP payload.
    pub fn seal(&mut self, aad: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(&self.counter.to_le_bytes());
        let sealed = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload { msg: payload, aad },
            )
            .expect("ChaCha20-Poly1305 seal cannot fail for a valid key");
        self.counter = self.counter.wrapping_add(1);

        let mut out = sealed;
        out.extend_from_slice(&nonce_bytes[4..]);
        out
    }

    /// The counter the next `seal` will use — exposed for the position/anchor
    /// bookkeeping and tests.
    pub fn counter(&self) -> u64 {
        self.counter
    }
}

/// The mapping an AirPlay 2 SETRATEANCHORTIME establishes: RTP timestamp `rtp`
/// is playing at network time `anchor_ntp`. Given that anchor and a target
/// network time, the RTP timestamp due then follows from the sample rate. Pure,
/// so the anchor math is tested without a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateAnchor {
    pub rtp_time: u32,
    pub anchor_ntp: super::rtp::NtpTime,
    pub sample_rate: u32,
}

impl RateAnchor {
    /// The RTP timestamp playing at `now` (NTP), advancing the anchor by the
    /// elapsed seconds × sample rate.
    pub fn rtp_at(&self, now: super::rtp::NtpTime) -> u32 {
        let anchor_secs = f64::from(self.anchor_ntp.seconds)
            + f64::from(self.anchor_ntp.fraction) / f64::from(u32::MAX);
        let now_secs = f64::from(now.seconds) + f64::from(now.fraction) / f64::from(u32::MAX);
        let elapsed = (now_secs - anchor_secs).max(0.0);
        self.rtp_time
            .wrapping_add((elapsed * f64::from(self.sample_rate)) as u32)
    }
}

/// Which clock an AirPlay 2 receiver uses, decided from its features — PTP for
/// receivers that advertise it, else the NTP timing responder RAOP already runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingProtocol {
    /// The receiver participates in the sender's NTP timing (RAOP-style).
    Ntp,
    /// The receiver requires PTP (IEEE 1588) clock synchronization.
    Ptp,
}

impl TimingProtocol {
    /// Bit 42 of the features value marks a receiver that requires PTP timing.
    const SUPPORTS_PTP: u64 = 1 << 42;

    /// Decide the timing protocol from a receiver's features value.
    pub fn from_features(features: u64) -> Self {
        if features & Self::SUPPORTS_PTP != 0 {
            TimingProtocol::Ptp
        } else {
            TimingProtocol::Ntp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airplay::rtp::NtpTime;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use x25519_dalek::{PublicKey, StaticSecret};

    /// A scripted fake AirPlay 2 receiver running the pair-verify *server* side
    /// independently of [`PairVerify`]: it generates its own ephemeral X25519 key
    /// and Ed25519 identity, derives the same session key from first principles,
    /// and checks the sender's M3 signature — so a completed exchange is evidence
    /// the sender interoperates with the reference math, not with itself.
    struct FakeReceiver {
        ephemeral: StaticSecret,
        ephemeral_public: PublicKey,
        signing: SigningKey,
        shared: Option<[u8; 32]>,
    }

    impl FakeReceiver {
        fn new() -> Self {
            let ephemeral = StaticSecret::random_from_rng(SecureRng);
            let ephemeral_public = PublicKey::from(&ephemeral);
            FakeReceiver {
                ephemeral,
                ephemeral_public,
                signing: SigningKey::generate(&mut SecureRng),
                shared: None,
            }
        }

        /// Given the sender's M1 (its ephemeral key), build M2.
        fn m2(&mut self, m1: &[u8]) -> Vec<u8> {
            let tlv = Tlv8::decode(m1).unwrap();
            assert_eq!(tlv.get_u8(tlv_type::STATE), Some(state::M1));
            let sender_pub: [u8; 32] = tlv.get(tlv_type::PUBLIC_KEY).unwrap().try_into().unwrap();

            let shared = self
                .ephemeral
                .diffie_hellman(&PublicKey::from(sender_pub))
                .to_bytes();
            self.shared = Some(shared);
            let session_key = hkdf32(
                b"Pair-Verify-Encrypt-Salt",
                &shared,
                b"Pair-Verify-Encrypt-Info",
            );
            let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));

            // Sign device_ephemeral || sender_ephemeral.
            let mut signed = Vec::new();
            signed.extend_from_slice(self.ephemeral_public.as_bytes());
            signed.extend_from_slice(&sender_pub);
            let signature = self.signing.sign(&signed);
            let inner = Tlv8::new()
                .push(tlv_type::IDENTIFIER, b"fake-receiver".to_vec())
                .push(tlv_type::SIGNATURE, signature.to_bytes().to_vec())
                .encode();
            let sealed = cipher
                .encrypt(
                    &pair_verify_nonce(b"PV-Msg02"),
                    Payload {
                        msg: &inner,
                        aad: &[],
                    },
                )
                .unwrap();

            Tlv8::new()
                .push_u8(tlv_type::STATE, state::M2)
                .push(
                    tlv_type::PUBLIC_KEY,
                    self.ephemeral_public.as_bytes().to_vec(),
                )
                .push(tlv_type::ENCRYPTED_DATA, sealed)
                .encode()
        }

        /// Verify the sender's M3 signature and return M4.
        fn m4(&self, m3: &[u8], sender_verifying: &VerifyingKey, sender_pub: &[u8; 32]) -> Vec<u8> {
            let tlv = Tlv8::decode(m3).unwrap();
            assert_eq!(tlv.get_u8(tlv_type::STATE), Some(state::M3));
            let encrypted = tlv.get(tlv_type::ENCRYPTED_DATA).unwrap();

            let session_key = hkdf32(
                b"Pair-Verify-Encrypt-Salt",
                self.shared.as_ref().unwrap(),
                b"Pair-Verify-Encrypt-Info",
            );
            let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));
            let inner = cipher
                .decrypt(&pair_verify_nonce(b"PV-Msg03"), encrypted)
                .expect("M3 decrypts with the shared session key");
            let inner_tlv = Tlv8::decode(&inner).unwrap();
            let sig_bytes: [u8; 64] = inner_tlv
                .get(tlv_type::SIGNATURE)
                .unwrap()
                .try_into()
                .unwrap();

            let mut signed = Vec::new();
            signed.extend_from_slice(sender_pub);
            signed.extend_from_slice(self.ephemeral_public.as_bytes());
            sender_verifying
                .verify(&signed, &Signature::from_bytes(&sig_bytes))
                .expect("sender M3 signature verifies against its ephemeral identity");

            Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode()
        }
    }

    /// The full pair-verify completes and both sides derive the same X25519
    /// shared secret, the fake receiver running the server independently.
    #[test]
    fn pair_verify_agrees_on_shared_secret() {
        let mut receiver = FakeReceiver::new();
        let mut verify = PairVerify::new();

        let m1 = verify.start().unwrap();
        let sender_pub = *verify.ephemeral_public.as_bytes();
        let sender_verifying = verify.signing.verifying_key();

        let m2 = receiver.m2(&m1);
        let m3 = verify.handle_m2(&m2).unwrap();
        let m4 = receiver.m4(&m3, &sender_verifying, &sender_pub);
        let sender_secret = verify.handle_m4(&m4).unwrap();

        assert_eq!(
            sender_secret,
            receiver.shared.unwrap(),
            "sender and independent receiver derive the same X25519 secret"
        );
    }

    /// An error TLV in M2 surfaces as a rejection, not a panic.
    #[test]
    fn receiver_error_is_surfaced() {
        let mut verify = PairVerify::new();
        verify.start().unwrap();
        let err = Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push_u8(tlv_type::ERROR, 0x02)
            .encode();
        assert_eq!(verify.handle_m2(&err), Err(PairingError::Rejected(0x02)));
    }

    /// The audio cipher round-trips: a packet sealed by the sender decrypts under
    /// the same key, nonce, and associated data, and the counter advances.
    #[test]
    fn audio_cipher_round_trips_and_advances_the_counter() {
        let shared = [0x5Au8; 32];
        let mut cipher = Ap2AudioCipher::from_shared_secret(&shared);
        assert_eq!(cipher.counter(), 0);

        let aad = [0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
        let payload = vec![9u8; 40];
        let sealed = cipher.seal(&aad, &payload);
        assert_eq!(cipher.counter(), 1);

        // The last 8 bytes are the nonce counter (0 for the first packet).
        let (ct, nonce_tail) = sealed.split_at(sealed.len() - 8);
        assert_eq!(nonce_tail, &0u64.to_le_bytes());
        assert_eq!(ct.len(), payload.len() + 16, "payload + Poly1305 tag");

        // Decrypt independently to prove the seal.
        let key = crate::airplay::ap2_channel::audio_key(&shared);
        let dec = ChaCha20Poly1305::new(Key::from_slice(&key));
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(nonce_tail);
        let plain = dec
            .decrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload { msg: ct, aad: &aad },
            )
            .unwrap();
        assert_eq!(plain, payload);
    }

    /// The rate anchor advances the RTP timestamp by elapsed × sample rate.
    #[test]
    fn rate_anchor_advances_with_elapsed_time() {
        let anchor = RateAnchor {
            rtp_time: 1000,
            anchor_ntp: NtpTime {
                seconds: 100,
                fraction: 0,
            },
            sample_rate: 44_100,
        };
        // One second later, 44100 more frames have played.
        let later = NtpTime {
            seconds: 101,
            fraction: 0,
        };
        assert_eq!(anchor.rtp_at(later), 1000 + 44_100);
        // At the anchor instant, the RTP time is exactly the anchor.
        assert_eq!(anchor.rtp_at(anchor.anchor_ntp), 1000);
    }

    /// PTP is chosen only when the receiver advertises the PTP feature bit.
    #[test]
    fn timing_protocol_follows_features() {
        assert_eq!(TimingProtocol::from_features(0), TimingProtocol::Ntp);
        assert_eq!(TimingProtocol::from_features(1 << 42), TimingProtocol::Ptp);
    }
}
