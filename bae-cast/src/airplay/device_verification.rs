//! AirPlay 1 device verification: how bae pairs once with an AirPlay 1
//! receiver that asks for a PIN, and proves at each connection that it is the
//! device the receiver paired with.
//!
//! The flow pyatv's legacy pairing (`auth/legacy.py`, `srp.py`) and OwnTone's
//! `pair_fruit.c` run, over the receiver's plaintext RTSP connection:
//!
//! 1. `POST /pair-pin-start`: the receiver shows a PIN.
//! 2. `POST /pair-setup-pin` with the binary plist `{method: "pin", user}`,
//!    `user` a random identifier. The receiver answers `{pk, salt}`.
//! 3. SRP-6a over the 2048-bit group with SHA-1, the identifier as the
//!    username and the PIN as the password, and the two-hash session key
//!    ([`super::srp::SessionKey::TwoHashes`]). bae sends `{pk: A, proof: M1}`
//!    and checks the receiver's `{proof: M2}`. A receiver that didn't take
//!    the PIN answers this step with HTTP 500 (pyatv issue 1548).
//! 4. bae sends `{epk, authTag}`: its Ed25519 public key sealed with
//!    AES-128-GCM under `SHA512("Pair-Setup-AES-Key" | K)[..16]` and a
//!    16-byte IV, `SHA512("Pair-Setup-AES-IV" | K)[..16]` with its last byte
//!    plus one.
//!
//! Each RTSP connection then starts with pair-verify:
//!
//! 1. `POST /pair-verify` with `01 00 00 00 | bae's X25519 ephemeral | bae's
//!    Ed25519 public key`. The receiver answers its X25519 ephemeral and 64
//!    more bytes.
//! 2. bae signs its ephemeral then the receiver's with its Ed25519 key, runs
//!    the receiver's 64 bytes and then the signature through one AES-128-CTR
//!    keystream under `SHA512("Pair-Verify-AES-Key" | shared)[..16]` and IV
//!    `SHA512("Pair-Verify-AES-IV" | shared)[..16]`, and sends
//!    `00 00 00 00 | the encrypted signature`. A receiver that refuses either
//!    step no longer knows bae's key (OwnTone drops its key on both).
//!
//! The verification proves bae and keys nothing: the audio stays as the
//! receiver's encryption types say. The receiver gives back no key, so bae
//! keeps only its own ([`VerificationKey`]).

use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use aes::Aes128;
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::AesGcm;
use rand::RngCore;
use sha2::{Digest, Sha512};
use x25519_dalek::{x25519, X25519_BASEPOINT_BYTES};

use super::bplist::{self, BplistError, Plist};
use super::pairing::{public_key, sign};
use super::pairing_keys::VerificationKey;
use super::rtsp::{Method, RtspConnection, RtspRequest};
use super::srp::{SrpClient, SrpError, SrpSuite};

/// The steps a receiver can refuse, as an error names them.
const PIN_START: &str = "/pair-pin-start";
const SETUP_USER: &str = "/pair-setup-pin user";
/// The step a receiver answers with 500 when the PIN is wrong.
const SETUP_PROOF: &str = "/pair-setup-pin proof";
const SETUP_KEY: &str = "/pair-setup-pin key";
const VERIFY_KEYS: &str = "/pair-verify keys";
const VERIFY_SIGNATURE: &str = "/pair-verify signature";

/// The status a receiver answers a wrong PIN with.
const WRONG_PIN_STATUS: u16 = 500;

/// The header that names this device to the person at the receiver.
const CLIENT_NAME: &str = "X-Apple-Client-Name";
const PLIST: &str = "application/x-apple-binary-plist";
const OCTETS: &str = "application/octet-stream";

/// An X25519 key or shared secret, and an Ed25519 public key, in bytes.
const KEY_LEN: usize = 32;
/// The bytes the receiver sends after its ephemeral key in pair-verify.
const RECEIVER_PROOF_LEN: usize = 64;
/// The AES-128 key and IV, and the GCM tag, in bytes.
const AES_LEN: usize = 16;

/// AES-128-GCM with the 16-byte IV the receiver expects.
type Aes128Gcm16 = AesGcm<Aes128, U16>;
type Aes128Ctr = ctr::Ctr128BE<Aes128>;

/// What each RAOP session to a receiver that verifies the device proves bae
/// with: the key the pairing gave the receiver, and the name the requests
/// give this device.
#[derive(Debug, Clone)]
pub(crate) struct SessionVerification {
    pub(crate) key: VerificationKey,
    pub(crate) client_name: String,
}

/// A failure pairing with an AirPlay 1 receiver or verifying bae to it.
#[derive(Debug, thiserror::Error)]
pub(crate) enum VerificationError {
    /// The connection failed, timed out, or closed.
    #[error("AirPlay 1 pairing connection error: {0}")]
    Io(#[from] std::io::Error),
    /// The receiver answered a step with a status that names no reason.
    #[error("receiver rejected {step} (status {status})")]
    Rejected { step: &'static str, status: u16 },
    /// The receiver didn't take the PIN.
    #[error("the receiver did not accept the PIN")]
    WrongPin,
    /// The receiver refused the key bae kept for it.
    #[error("the receiver no longer knows this device")]
    NoLongerKnown,
    /// A reply is not a binary plist bae can read.
    #[error("the receiver's reply to {step} is not a readable plist: {error}")]
    Plist { step: &'static str, error: BplistError },
    /// A reply lacks what the step needs, or has a size no valid reply has.
    #[error("the receiver's reply to {step} has no valid {field}")]
    BadReply { step: &'static str, field: &'static str },
    /// The SRP exchange failed: the receiver's public value or its proof.
    #[error("SRP failure: {0:?}")]
    Srp(SrpError),
}

impl From<SrpError> for VerificationError {
    fn from(error: SrpError) -> Self {
        VerificationError::Srp(error)
    }
}

/// Ask the receiver to show a PIN, for a pairing with that PIN. The receiver
/// shows `client_name` beside it.
pub(crate) fn show_pin(
    conn: &mut RtspConnection,
    client_name: &str,
) -> Result<(), VerificationError> {
    let response =
        conn.request(&RtspRequest::new(Method::Post, PIN_START).header(CLIENT_NAME, client_name))?;
    if response.is_success() {
        Ok(())
    } else {
        Err(VerificationError::Rejected { step: PIN_START, status: response.status })
    }
}

/// Pair with the receiver over the `pin` it shows, giving it `key`'s public
/// half, which every later connection proves. A receiver that answers the
/// proof with 500 didn't take the PIN.
pub(crate) fn pair_with_pin(
    conn: &mut RtspConnection,
    client_name: &str,
    pin: &str,
    key: &VerificationKey,
) -> Result<(), VerificationError> {
    let pairing = PinPairing::new(pin, key);
    let salt = post(conn, "/pair-setup-pin", SETUP_USER, PLIST, client_name, pairing.start())?;
    let (proof, client) = pairing.answer_salt(&salt)?;
    let confirmed = match post(conn, "/pair-setup-pin", SETUP_PROOF, PLIST, client_name, proof) {
        Err(VerificationError::Rejected { status: WRONG_PIN_STATUS, .. }) => {
            return Err(VerificationError::WrongPin);
        }
        replied => replied?,
    };
    let sealed_key = pairing.answer_proof(&confirmed, &client)?;
    post(conn, "/pair-setup-pin", SETUP_KEY, PLIST, client_name, sealed_key)?;
    Ok(())
}

/// Prove to the receiver that this connection comes from the device that
/// paired with `key`. A receiver that refuses either step no longer knows
/// the key.
pub(crate) fn verify(
    conn: &mut RtspConnection,
    client_name: &str,
    key: &VerificationKey,
) -> Result<(), VerificationError> {
    let verify = PairVerify::new(key);
    let known = |error| match error {
        VerificationError::Rejected { .. } => VerificationError::NoLongerKnown,
        other => other,
    };
    let reply = post(conn, "/pair-verify", VERIFY_KEYS, OCTETS, client_name, verify.start())
        .map_err(known)?;
    let signature = verify.answer(&reply)?;
    post(conn, "/pair-verify", VERIFY_SIGNATURE, OCTETS, client_name, signature).map_err(known)?;
    Ok(())
}

/// POST `body` to `path` as the step `step`, and return the reply's body. A
/// status other than 2xx is the step's rejection.
fn post(
    conn: &mut RtspConnection,
    path: &str,
    step: &'static str,
    content_type: &str,
    client_name: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, VerificationError> {
    let response = conn.request(
        &RtspRequest::with_body(Method::Post, path, content_type, body)
            .header(CLIENT_NAME, client_name),
    )?;
    if response.is_success() {
        Ok(response.body)
    } else {
        Err(VerificationError::Rejected { step, status: response.status })
    }
}

/// A pairing over the PIN the receiver shows, as bodies: built, then each
/// reply answered in turn.
struct PinPairing<'k> {
    /// The SRP username, a random identifier in capital hex (pyatv's 8
    /// bytes).
    user: String,
    pin: Vec<u8>,
    key: &'k VerificationKey,
}

impl<'k> PinPairing<'k> {
    fn new(pin: &str, key: &'k VerificationKey) -> Self {
        let mut user = [0u8; 8];
        rand::rng().fill_bytes(&mut user);
        PinPairing { user: hex::encode_upper(user), pin: pin.as_bytes().to_vec(), key }
    }

    /// The first body: the method and the username.
    fn start(&self) -> Vec<u8> {
        bplist::encode(&Plist::Dict(vec![
            ("method".to_string(), Plist::String("pin".to_string())),
            ("user".to_string(), Plist::String(self.user.clone())),
        ]))
    }

    /// Run SRP over the receiver's `{pk, salt}` and return the body with bae's
    /// public value and proof, with the exchange that checks the receiver's.
    fn answer_salt(&self, reply: &[u8]) -> Result<(Vec<u8>, SrpClient), VerificationError> {
        let reply = read_plist(SETUP_USER, reply)?;
        let b_pub = data_field(&reply, SETUP_USER, "pk")?;
        let salt = data_field(&reply, SETUP_USER, "salt")?;
        let mut a = [0u8; 32];
        rand::rng().fill_bytes(&mut a);
        let mut client = SrpClient::new(
            SrpSuite::airplay1(),
            self.user.as_bytes(),
            &self.pin,
            num_bigint::BigUint::from_bytes_be(&a),
        );
        client.process(salt, b_pub)?;
        let body = bplist::encode(&Plist::Dict(vec![
            ("pk".to_string(), Plist::Data(client.public_a())),
            ("proof".to_string(), Plist::Data(client.proof_m1()?.to_vec())),
        ]));
        Ok((body, client))
    }

    /// Check the receiver's proof and return the body that gives it bae's
    /// public key, sealed under the SRP key.
    fn answer_proof(&self, reply: &[u8], client: &SrpClient) -> Result<Vec<u8>, VerificationError> {
        let reply = read_plist(SETUP_PROOF, reply)?;
        client.verify_m2(data_field(&reply, SETUP_PROOF, "proof")?)?;

        let srp_key = client.shared_key()?;
        let aes_key = labelled_hash(b"Pair-Setup-AES-Key", srp_key);
        let mut iv = labelled_hash(b"Pair-Setup-AES-IV", srp_key);
        iv[AES_LEN - 1] = iv[AES_LEN - 1].wrapping_add(1);
        let mut sealed = Aes128Gcm16::new(&aes_key.into())
            .encrypt(&iv.into(), public_key(&self.key.seed).as_slice())
            .expect("AES-GCM seals a 32-byte key under any key and IV");
        let tag = sealed.split_off(sealed.len() - AES_LEN);
        Ok(bplist::encode(&Plist::Dict(vec![
            ("epk".to_string(), Plist::Data(sealed)),
            ("authTag".to_string(), Plist::Data(tag)),
        ])))
    }
}

/// A pair-verify over the kept key, as bodies, with a fresh X25519 ephemeral.
struct PairVerify<'k> {
    key: &'k VerificationKey,
    secret: [u8; KEY_LEN],
    public: [u8; KEY_LEN],
}

impl<'k> PairVerify<'k> {
    fn new(key: &'k VerificationKey) -> Self {
        let mut secret = [0u8; KEY_LEN];
        rand::rng().fill_bytes(&mut secret);
        PairVerify { key, secret, public: x25519(secret, X25519_BASEPOINT_BYTES) }
    }

    /// The first body: the marker, bae's ephemeral and its Ed25519 public key.
    fn start(&self) -> Vec<u8> {
        [&[1, 0, 0, 0][..], &self.public, &public_key(&self.key.seed)].concat()
    }

    /// Answer the receiver's ephemeral and proof with bae's encrypted
    /// signature over the two ephemerals.
    fn answer(&self, reply: &[u8]) -> Result<Vec<u8>, VerificationError> {
        let bad = |field| VerificationError::BadReply { step: VERIFY_KEYS, field };
        let (receiver_public, rest) =
            reply.split_first_chunk::<KEY_LEN>().ok_or_else(|| bad("public key"))?;
        if rest.len() < RECEIVER_PROOF_LEN {
            return Err(bad("proof"));
        }
        let shared = x25519(self.secret, *receiver_public);
        // A low-order point gives an all-zero secret any party can compute.
        if shared == [0u8; KEY_LEN] {
            return Err(bad("public key"));
        }
        let mut signature: [u8; 64] =
            sign(&self.key.seed, &[&self.public, &receiver_public[..]].concat())
                .try_into()
                .expect("an Ed25519 signature is 64 bytes");
        let mut cipher = Aes128Ctr::new(
            &labelled_hash(b"Pair-Verify-AES-Key", &shared).into(),
            &labelled_hash(b"Pair-Verify-AES-IV", &shared).into(),
        );
        // The receiver's 64 bytes took the keystream's start; bae's
        // signature takes what follows.
        cipher.seek(RECEIVER_PROOF_LEN as u64);
        cipher.apply_keystream(&mut signature);
        Ok([&[0, 0, 0, 0][..], &signature].concat())
    }
}

/// The first 16 bytes of `SHA512(label | secret)`: an AES-128 key or IV.
fn labelled_hash(label: &[u8], secret: &[u8]) -> [u8; AES_LEN] {
    let digest = Sha512::new().chain_update(label).chain_update(secret).finalize();
    let mut out = [0u8; AES_LEN];
    out.copy_from_slice(&digest[..AES_LEN]);
    out
}

fn read_plist(step: &'static str, reply: &[u8]) -> Result<Plist, VerificationError> {
    bplist::decode(reply).map_err(|error| VerificationError::Plist { step, error })
}

fn data_field<'p>(
    reply: &'p Plist,
    step: &'static str,
    field: &'static str,
) -> Result<&'p [u8], VerificationError> {
    reply.get(field).and_then(Plist::as_data).ok_or(VerificationError::BadReply { step, field })
}

#[cfg(test)]
#[path = "device_verification_tests.rs"]
mod tests;
