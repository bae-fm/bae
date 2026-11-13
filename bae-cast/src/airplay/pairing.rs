//! HomeKit transient pairing — the password-less pair-setup an AirPlay 2 sender
//! runs before it can stream.
//!
//! Transient pairing is the first four states (M1–M4) of HomeKit pair-setup with
//! the transient flag set: no PIN, no persistent identity stored. The SRP shared
//! secret it establishes is what the control-channel and audio-stream encryption
//! keys derive from. This is the flow pyatv's `hap_transient` (MIT, `master` @
//! 2024) runs against real HomePods:
//!
//! 1. `POST /pair-setup` with `{Method=0, State=M1, Flags=Transient}`.
//! 2. Receiver replies `{State=M2, Salt, PublicKey=B}`.
//! 3. `POST /pair-setup` with `{State=M3, PublicKey=A, Proof=M1}`.
//! 4. Receiver replies `{State=M4, Proof=M2}`; the shared secret is the SRP
//!    session key. The control connection is encrypted from here on with keys
//!    derived from it; no pair-verify follows.
//!
//! This module is the transport-agnostic state machine: it produces each request
//! *body* (TLV8) and consumes each response *body*, so the session drives the
//! POSTs over the socket and the tests drive it against a scripted fake receiver.
//! The SRP username/password are the HomeKit transient constants.

use rand::RngCore;

use super::srp::{sha512, SrpClient, SrpError, SrpGroup};
use super::tlv8::{state, tlv_type, Tlv8, Tlv8Error, FLAG_TRANSIENT};
use super::{check_no_error, expect_state};

/// The SRP identity for transient pair-setup — a fixed username and PIN every
/// receiver accepts (pyatv `TRANSIENT_PIN`).
const PAIR_SETUP_USERNAME: &[u8] = b"Pair-Setup";
const TRANSIENT_PIN: &[u8] = b"3939";

/// The SRP salt a receiver sends, in bytes.
const SALT_LEN: usize = 16;
/// The most bytes the receiver's SRP public value takes: the 3072-bit group.
const MAX_PUBLIC_KEY_LEN: usize = 384;
/// The receiver's SRP proof, in bytes: a SHA-512 digest.
const SERVER_PROOF_LEN: usize = 64;

/// The `X-Apple-HKP` value on every transient `/pair-setup` POST: 4 marks the
/// transient flow. Receivers read nothing else from a request without it.
pub(crate) const TRANSIENT_HKP: &str = "4";

/// The transient pair-setup state machine. Built, then driven M1→M3, yielding the
/// SRP shared secret once M4 arrives.
pub(crate) struct TransientPairing {
    phase: PairSetupPhase,
}

enum PairSetupPhase {
    /// Nothing sent yet.
    Init,
    /// M1 sent; waiting for the receiver's M2 (salt + B).
    AwaitingM2,
    /// M3 sent; the SRP exchange is complete and holds the shared secret.
    AwaitingM4(Box<SrpClient>),
    /// M4 verified; the flow is done.
    Done,
}

impl TransientPairing {
    pub(crate) fn new() -> Self {
        TransientPairing { phase: PairSetupPhase::Init }
    }

    /// The body of the first `POST /pair-setup` (M1): method 0, state M1, and the
    /// transient flag.
    pub(crate) fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, PairSetupPhase::Init) {
            return Err(PairingError::WrongState);
        }
        self.phase = PairSetupPhase::AwaitingM2;
        Ok(Tlv8::new()
            .push_u8(tlv_type::METHOD, 0)
            .push_u8(tlv_type::STATE, state::M1)
            .push(tlv_type::FLAGS, FLAG_TRANSIENT.to_le_bytes()[..1].to_vec())
            .encode())
    }

    /// Handle the receiver's M2 (salt + B): run the SRP exchange and return the
    /// M3 body (`{State=M3, PublicKey=A, Proof=M1}`) to POST.
    pub(crate) fn handle_m2(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, PairSetupPhase::AwaitingM2) {
            return Err(PairingError::WrongState);
        }
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M2)?;

        let salt = tlv.get(tlv_type::SALT).ok_or(PairingError::MissingField("salt"))?;
        let b_pub =
            tlv.get(tlv_type::PUBLIC_KEY).ok_or(PairingError::MissingField("public key"))?;
        // The sizes pair_ap holds a receiver to: a 16-byte salt and a public
        // value that fits the 3072-bit group. B of 0 mod N is the SRP step's.
        if salt.len() != SALT_LEN {
            return Err(PairingError::InvalidField("salt length"));
        }
        if b_pub.len() > MAX_PUBLIC_KEY_LEN {
            return Err(PairingError::InvalidField("public key length"));
        }

        let mut a = [0u8; 32];
        rand::rng().fill_bytes(&mut a);
        let mut client = SrpClient::new(
            SrpGroup::rfc5054_3072(),
            sha512,
            PAIR_SETUP_USERNAME,
            TRANSIENT_PIN,
            num_bigint::BigUint::from_bytes_be(&a),
        );
        client.process(salt, b_pub)?;

        let m3 = Tlv8::new()
            .push_u8(tlv_type::STATE, state::M3)
            .push(tlv_type::PUBLIC_KEY, client.public_a())
            .push(tlv_type::PROOF, client.proof_m1()?.to_vec())
            .encode();

        self.phase = PairSetupPhase::AwaitingM4(Box::new(client));
        Ok(m3)
    }

    /// Handle the receiver's M4: verify its server proof, which must be there,
    /// and return the SRP shared secret the channel and stream keys derive from.
    pub(crate) fn handle_m4(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        let PairSetupPhase::AwaitingM4(client) =
            std::mem::replace(&mut self.phase, PairSetupPhase::Done)
        else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M4)?;

        // Transient M4 carries the server proof (M2), 64 bytes of SHA-512. A
        // receiver that leaves it out has not shown it holds the key; pair_ap,
        // which pairs real HomePods for OwnTone, requires it.
        let server_proof = tlv.get(tlv_type::PROOF).ok_or(PairingError::MissingField("proof"))?;
        if server_proof.len() != SERVER_PROOF_LEN {
            return Err(PairingError::InvalidField("proof length"));
        }
        client.verify_m2(server_proof)?;
        Ok(client.shared_key()?.to_vec())
    }
}

/// A failure during transient pairing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PairingError {
    /// A method was called out of sequence.
    #[error("pairing step called out of sequence")]
    WrongState,
    /// The receiver's TLV8 was malformed.
    #[error("malformed pairing message: {0}")]
    Tlv(#[from] Tlv8Error),
    /// A required TLV field was missing from a response.
    #[error("pairing response missing {0}")]
    MissingField(&'static str),
    /// The receiver's state byte was not the expected step.
    #[error("pairing state {actual:?}, expected {expected}")]
    UnexpectedState { expected: u8, actual: Option<u8> },
    /// A field of the receiver's response has a size no valid reply has.
    #[error("pairing response has an invalid {0}")]
    InvalidField(&'static str),
    /// The receiver returned an Error TLV.
    #[error("receiver refused pairing: {0}")]
    Receiver(HomeKitError),
    /// The SRP exchange failed (bad server public value or proof). `SrpError` is
    /// a plain result enum, not an `Error`, so this carries it by `From` without
    /// making it this error's `source()`.
    #[error("SRP failure: {0:?}")]
    Srp(SrpError),
}

/// The HomeKit error a receiver answers a pairing step with (the Error TLV;
/// HAP specification, table 5-5), so what it said reaches the person who cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum HomeKitError {
    #[error("unknown error")]
    Unknown,
    /// The receiver doesn't accept the proof or the controller.
    #[error("authentication failed")]
    Authentication,
    /// The receiver asks the sender to wait before trying again.
    #[error("asks to wait {seconds} seconds")]
    BackOff { seconds: u64 },
    /// The receiver is paired with as many controllers as it allows.
    #[error("paired with as many devices as it allows")]
    MaxPeers,
    /// The receiver saw too many failed attempts and refuses more.
    #[error("refuses more pairing attempts")]
    MaxTries,
    /// The receiver isn't accepting new pairings.
    #[error("not accepting new pairings")]
    Unavailable,
    /// The receiver is pairing with another controller.
    #[error("pairing with another device")]
    Busy,
    /// A code outside the specification's.
    #[error("error code {0}")]
    Other(u8),
}

impl HomeKitError {
    /// The error `code` names in `reply`, which carries the delay a BackOff
    /// states. A BackOff without a readable delay is a malformed reply: no
    /// wait can be worded for it.
    pub(crate) fn from_reply(code: u8, reply: &Tlv8) -> Result<Self, PairingError> {
        Ok(match code {
            0x01 => HomeKitError::Unknown,
            0x02 => HomeKitError::Authentication,
            0x03 => {
                let delay = reply
                    .get(tlv_type::RETRY_DELAY)
                    .ok_or(PairingError::MissingField("retry delay"))?;
                let bytes: [u8; 8] = {
                    let mut padded = [0u8; 8];
                    let low = padded
                        .get_mut(..delay.len())
                        .ok_or(PairingError::InvalidField("retry delay length"))?;
                    low.copy_from_slice(delay);
                    padded
                };
                HomeKitError::BackOff { seconds: u64::from_le_bytes(bytes) }
            }
            0x04 => HomeKitError::MaxPeers,
            0x05 => HomeKitError::MaxTries,
            0x06 => HomeKitError::Unavailable,
            0x07 => HomeKitError::Busy,
            other => HomeKitError::Other(other),
        })
    }
}

impl From<SrpError> for PairingError {
    fn from(e: SrpError) -> Self {
        PairingError::Srp(e)
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
