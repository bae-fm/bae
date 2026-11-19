//! HomeKit pair-setup, the pairing an AirPlay 2 sender runs before it can
//! stream: the password-less transient pairing, and the pairing with a code
//! the receiver asks for.
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
//! Pairing with a code runs the same SRP exchange without the flag, over the
//! receiver's PIN or password, then exchanges long-term Ed25519 identities
//! (M5 and M6, pyatv's `hap_srp` steps 3 and 4 and OwnTone's
//! `client_setup_request3`/`client_setup_response3`). What it yields is the
//! [`PairingKeys`] each later session pair-verifies with.
//!
//! This module is the transport-agnostic state machine: it produces each request
//! *body* (TLV8) and consumes each response *body*, so the session drives the
//! POSTs over the socket and the tests drive it against a scripted fake receiver.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::RngCore;

use super::ap2_channel::{hkdf32, nonce_with_tail};
use super::pairing_keys::{ControllerIdentity, PairingKeys};
use super::srp::{SrpClient, SrpError, SrpSuite};
use super::tlv8::{state, tlv_type, Tlv8, Tlv8Error, FLAG_TRANSIENT};
use super::{check_no_error, expect_state};

/// The SRP username of every HomeKit pair-setup, and the PIN of the transient
/// one, which every receiver accepts (pyatv `TRANSIENT_PIN`).
const PAIR_SETUP_USERNAME: &[u8] = b"Pair-Setup";
const TRANSIENT_PIN: &[u8] = b"3939";

/// The SRP salt a receiver sends, in bytes.
const SALT_LEN: usize = 16;
/// The most bytes the receiver's SRP public value takes: the 3072-bit group.
const MAX_PUBLIC_KEY_LEN: usize = 384;
/// The receiver's SRP proof, in bytes: a SHA-512 digest.
const SERVER_PROOF_LEN: usize = 64;
/// An Ed25519 public key, and a signature, in bytes.
pub(crate) const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// The `X-Apple-HKP` value on every transient `/pair-setup` POST: 4 marks the
/// transient flow. Receivers read nothing else from a request without it.
pub(crate) const TRANSIENT_HKP: &str = "4";
/// The `X-Apple-HKP` value on `/pair-pin-start` and on every `/pair-setup` and
/// `/pair-verify` POST of a pairing with a code: 3, as OwnTone sends for its
/// normal pairing and pyatv for every HomeKit pairing.
pub(crate) const CODE_HKP: &str = "3";

/// Seal `plaintext` under `key` with ChaCha20-Poly1305, the nonce four zero
/// bytes then the 8-byte message `label` (`PS-Msg05`), no associated data:
/// ciphertext, then the 16-byte tag.
pub(crate) fn seal(key: &[u8; 32], label: &[u8; 8], plaintext: &[u8]) -> Vec<u8> {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(&nonce_with_tail(label), plaintext)
        .expect("ChaCha20-Poly1305 seal cannot fail for a valid key")
}

/// Open what [`seal`] sealed. A tag that doesn't verify is a reply that
/// doesn't check out.
pub(crate) fn open(
    key: &[u8; 32],
    label: &[u8; 8],
    sealed: &[u8],
) -> Result<Vec<u8>, PairingError> {
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(&nonce_with_tail(label), sealed)
        .map_err(PairingError::Decrypt)
}

/// Check `signature` over `message` with the Ed25519 `public_key`.
pub(crate) fn verify_signature(
    public_key: &[u8; ED25519_PUBLIC_KEY_LEN],
    message: &[u8],
    signature: &[u8],
) -> Result<(), PairingError> {
    let signature = Signature::from_slice(signature).map_err(PairingError::MalformedSignature)?;
    let key = VerifyingKey::from_bytes(public_key).map_err(PairingError::MalformedPublicKey)?;
    key.verify_strict(message, &signature).map_err(PairingError::BadSignature)
}

/// Sign `message` with the Ed25519 key `seed` makes.
pub(crate) fn sign(seed: &[u8; 32], message: &[u8]) -> Vec<u8> {
    SigningKey::from_bytes(seed).sign(message).to_bytes().to_vec()
}

/// The Ed25519 public key of the key `seed` makes.
pub(crate) fn public_key(seed: &[u8; 32]) -> [u8; ED25519_PUBLIC_KEY_LEN] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// Read the receiver's M2 (salt + B), run the SRP exchange over `password`,
/// and return the M3 body (`{State=M3, PublicKey=A, Proof=M1}`) with the
/// exchange that checks M4.
fn answer_m2(
    response_body: &[u8],
    password: &[u8],
) -> Result<(Vec<u8>, Box<SrpClient>), PairingError> {
    let tlv = Tlv8::decode(response_body)?;
    check_no_error(&tlv)?;
    expect_state(&tlv, state::M2)?;

    let salt = tlv.get(tlv_type::SALT).ok_or(PairingError::MissingField("salt"))?;
    let b_pub = tlv.get(tlv_type::PUBLIC_KEY).ok_or(PairingError::MissingField("public key"))?;
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
        SrpSuite::homekit(),
        PAIR_SETUP_USERNAME,
        password,
        num_bigint::BigUint::from_bytes_be(&a),
    );
    client.process(salt, b_pub)?;

    let m3 = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M3)
        .push(tlv_type::PUBLIC_KEY, client.public_a())
        .push(tlv_type::PROOF, client.proof_m1()?.to_vec())
        .encode();
    Ok((m3, Box::new(client)))
}

/// Read the receiver's M4: verify its server proof, which must be there, and
/// return the SRP shared secret. A receiver's error has already been read
/// from `tlv`.
fn read_m4(client: &SrpClient, tlv: &Tlv8) -> Result<Vec<u8>, PairingError> {
    expect_state(tlv, state::M4)?;
    // M4 carries the server proof (M2), 64 bytes of SHA-512. A receiver that
    // leaves it out has not shown it holds the key; pair_ap, which pairs real
    // HomePods for OwnTone, requires it.
    let server_proof = tlv.get(tlv_type::PROOF).ok_or(PairingError::MissingField("proof"))?;
    if server_proof.len() != SERVER_PROOF_LEN {
        return Err(PairingError::InvalidField("proof length"));
    }
    client.verify_m2(server_proof)?;
    Ok(client.shared_key()?.to_vec())
}

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
        let (m3, client) = answer_m2(response_body, TRANSIENT_PIN)?;
        self.phase = PairSetupPhase::AwaitingM4(client);
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
        read_m4(&client, &tlv)
    }
}

/// Pair-setup with the code a receiver asks for: its PIN's digits, or its
/// password. Built, then driven M1→M5, yielding the [`PairingKeys`] once M6
/// arrives.
pub(crate) struct CodePairing {
    code: Vec<u8>,
    identity: ControllerIdentity,
    phase: CodePhase,
}

enum CodePhase {
    /// Nothing sent yet.
    Init,
    /// M1 sent; waiting for the receiver's M2 (salt + B).
    AwaitingM2,
    /// M3 sent; waiting for the receiver's proof.
    AwaitingM4(Box<SrpClient>),
    /// M5 sent, bae's identity sealed under the SRP key; waiting for the
    /// receiver's.
    AwaitingM6 { srp_key: Vec<u8> },
    /// M6 read; the flow is done.
    Done,
}

impl CodePairing {
    /// A pairing over `code` — the PIN's digits as typed, leading zeros kept,
    /// or the password's UTF-8 — that introduces bae as `identity`.
    pub(crate) fn new(code: &str, identity: ControllerIdentity) -> Self {
        CodePairing { code: code.as_bytes().to_vec(), identity, phase: CodePhase::Init }
    }

    /// The body of the first `POST /pair-setup` (M1): method 0 and state M1,
    /// with no flags.
    pub(crate) fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, CodePhase::Init) {
            return Err(PairingError::WrongState);
        }
        self.phase = CodePhase::AwaitingM2;
        Ok(Tlv8::new().push_u8(tlv_type::METHOD, 0).push_u8(tlv_type::STATE, state::M1).encode())
    }

    /// Handle the receiver's M2 and return the M3 body, the SRP exchange run
    /// over the code.
    pub(crate) fn handle_m2(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, CodePhase::AwaitingM2) {
            return Err(PairingError::WrongState);
        }
        let (m3, client) = answer_m2(response_body, &self.code)?;
        self.phase = CodePhase::AwaitingM4(client);
        Ok(m3)
    }

    /// Handle the receiver's M4 and return the M5 body: bae's identifier, its
    /// Ed25519 public key and its signature over them, sealed under the SRP
    /// key. A receiver that answers M3 with Authentication didn't take the
    /// code.
    pub(crate) fn handle_m4(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        let CodePhase::AwaitingM4(client) = std::mem::replace(&mut self.phase, CodePhase::Done)
        else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        let retry_after_seconds = retry_delay(&tlv)?;
        check_no_error(&tlv).map_err(|error| match error {
            PairingError::Receiver(HomeKitError::Authentication) => {
                PairingError::WrongCode { retry_after_seconds }
            }
            other => other,
        })?;
        let srp_key = read_m4(&client, &tlv)?;

        let controller_x = hkdf32(
            b"Pair-Setup-Controller-Sign-Salt",
            &srp_key,
            b"Pair-Setup-Controller-Sign-Info",
        );
        let id = self.identity.id.as_bytes();
        let public = public_key(&self.identity.seed);
        let signature = sign(&self.identity.seed, &[&controller_x[..], id, &public].concat());
        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, id.to_vec())
            .push(tlv_type::PUBLIC_KEY, public.to_vec())
            .push(tlv_type::SIGNATURE, signature)
            .encode();
        let m5 = Tlv8::new()
            .push_u8(tlv_type::STATE, state::M5)
            .push(tlv_type::ENCRYPTED_DATA, seal(&setup_key(&srp_key), b"PS-Msg05", &inner))
            .encode();
        self.phase = CodePhase::AwaitingM6 { srp_key };
        Ok(m5)
    }

    /// Handle the receiver's M6: open its identity, check its signature, and
    /// return the keys every later session pair-verifies with.
    pub(crate) fn handle_m6(&mut self, response_body: &[u8]) -> Result<PairingKeys, PairingError> {
        let CodePhase::AwaitingM6 { srp_key } = std::mem::replace(&mut self.phase, CodePhase::Done)
        else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv)?;
        expect_state(&tlv, state::M6)?;
        let sealed = tlv
            .get(tlv_type::ENCRYPTED_DATA)
            .ok_or(PairingError::MissingField("encrypted data"))?;
        let inner = Tlv8::decode(&open(&setup_key(&srp_key), b"PS-Msg06", sealed)?)?;

        let receiver_id =
            inner.get(tlv_type::IDENTIFIER).ok_or(PairingError::MissingField("identifier"))?;
        let receiver_public =
            inner.get(tlv_type::PUBLIC_KEY).ok_or(PairingError::MissingField("public key"))?;
        let Ok(receiver_public) = <[u8; ED25519_PUBLIC_KEY_LEN]>::try_from(receiver_public) else {
            return Err(PairingError::InvalidField("public key length"));
        };
        let signature =
            inner.get(tlv_type::SIGNATURE).ok_or(PairingError::MissingField("signature"))?;
        let accessory_x =
            hkdf32(b"Pair-Setup-Accessory-Sign-Salt", &srp_key, b"Pair-Setup-Accessory-Sign-Info");
        verify_signature(
            &receiver_public,
            &[&accessory_x[..], receiver_id, &receiver_public].concat(),
            signature,
        )?;

        Ok(PairingKeys {
            controller: self.identity.clone(),
            receiver_id: receiver_id.to_vec(),
            receiver_public,
        })
    }
}

/// The key M5 and M6 are sealed under.
fn setup_key(srp_key: &[u8]) -> [u8; 32] {
    hkdf32(b"Pair-Setup-Encrypt-Salt", srp_key, b"Pair-Setup-Encrypt-Info")
}

/// A failure during pair-setup or pair-verify.
#[derive(Debug, thiserror::Error)]
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
    /// The receiver didn't take the PIN or password it was given. A receiver
    /// may state how long to wait before the next try (HomePod OS 27 does);
    /// a pair-setup sent sooner gets BackOff.
    #[error("the receiver did not accept the code")]
    WrongCode { retry_after_seconds: Option<u64> },
    /// The receiver no longer knows the identity bae kept for it: it refused
    /// pair-verify, or it answered as another identity than the one it paired
    /// as, which a receiver that was reset does.
    #[error("the receiver no longer knows this device")]
    PairingLost,
    /// A sealed message didn't open: its tag didn't verify.
    #[error("a sealed pairing message did not open: {0}")]
    Decrypt(chacha20poly1305::Error),
    /// The receiver's signature is not an Ed25519 signature's 64 bytes.
    #[error("the receiver's pairing signature is malformed: {0}")]
    MalformedSignature(ed25519_dalek::SignatureError),
    /// The receiver's public key is not an Ed25519 point.
    #[error("the receiver's pairing public key is malformed: {0}")]
    MalformedPublicKey(ed25519_dalek::SignatureError),
    /// The receiver's Ed25519 signature didn't verify.
    #[error("the receiver's pairing signature did not verify: {0}")]
    BadSignature(ed25519_dalek::SignatureError),
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
            0x03 => HomeKitError::BackOff {
                seconds: retry_delay(reply)?.ok_or(PairingError::MissingField("retry delay"))?,
            },
            0x04 => HomeKitError::MaxPeers,
            0x05 => HomeKitError::MaxTries,
            0x06 => HomeKitError::Unavailable,
            0x07 => HomeKitError::Busy,
            other => HomeKitError::Other(other),
        })
    }
}

/// The seconds `reply`'s RetryDelay item asks the sender to wait, which a
/// BackOff always states and a wrong code's Authentication may state; `None`
/// when it has none.
fn retry_delay(reply: &Tlv8) -> Result<Option<u64>, PairingError> {
    let Some(delay) = reply.get(tlv_type::RETRY_DELAY) else {
        return Ok(None);
    };
    let mut padded = [0u8; 8];
    padded
        .get_mut(..delay.len())
        .ok_or(PairingError::InvalidField("retry delay length"))?
        .copy_from_slice(delay);
    Ok(Some(u64::from_le_bytes(padded)))
}

impl From<SrpError> for PairingError {
    fn from(e: SrpError) -> Self {
        PairingError::Srp(e)
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
