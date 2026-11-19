//! The pairing a receiver that asked for a code and bae agreed on: the
//! identities each later session's pair-verify proves, the password a
//! receiver that has one asks for again on each session, the key an AirPlay 1
//! receiver verifies bae with, and the text bae keeps them as.
//!
//! bae-cast owns this text's format. Whoever keeps it stores the string by
//! name and never reads inside it.

use rand::RngCore;

use super::pairing::{public_key, ED25519_PUBLIC_KEY_LEN};

/// The name of the keychain entry in which this device keeps its pairing
/// with the receiver `receiver_id` (its MAC, capitals, no colons): one entry
/// per receiver, so replacing one pairing touches no other.
pub(crate) fn pairing_secret_name(receiver_id: &str) -> String {
    format!("airplay-pairing:{receiver_id}")
}

/// The name of the keychain entry in which this device keeps the key an
/// AirPlay 1 receiver `receiver_id` verifies it with, one entry per receiver.
/// AirPlay 1 device verification keeps another kind of pairing than AirPlay 2
/// does, so the two never share an entry.
pub(crate) fn verification_secret_name(receiver_id: &str) -> String {
    format!("airplay1-verification:{receiver_id}")
}

/// Who bae is to one receiver: the pairing identifier it introduced itself
/// with and the seed of the Ed25519 key it signs with. Each receiver gets an
/// identity of its own, so replacing one pairing touches no other.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ControllerIdentity {
    /// The pairing identifier, a UUID's text, as pyatv makes one.
    pub(crate) id: String,
    /// The 32-byte seed of bae's Ed25519 key for this receiver.
    pub(crate) seed: [u8; 32],
}

impl ControllerIdentity {
    /// A new identity for one receiver: an identifier from `ids`, a UUID's
    /// text in capitals as pyatv makes one, and a random seed.
    pub(crate) fn new(ids: &dyn coven::IdSource) -> Self {
        let mut seed = [0u8; 32];
        rand::rng().fill_bytes(&mut seed);
        ControllerIdentity { id: ids.new_id().to_string().to_uppercase(), seed }
    }
}

/// The seed is a secret: debug output names the identifier and the public
/// key only.
impl std::fmt::Debug for ControllerIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControllerIdentity")
            .field("id", &self.id)
            .field("public_key", &hex::encode(public_key(&self.seed)))
            .finish_non_exhaustive()
    }
}

/// Everything pair-verify needs: bae's identity for the receiver, and the
/// receiver's pairing identifier and Ed25519 public key as pair-setup's M6
/// gave them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairingKeys {
    pub(crate) controller: ControllerIdentity,
    pub(crate) receiver_id: Vec<u8>,
    pub(crate) receiver_public: [u8; ED25519_PUBLIC_KEY_LEN],
}

/// What bae keeps for one receiver: the keys pair-verify proves, and, for a
/// receiver with a password, that password, which it asks for again as HTTP
/// Digest on each session after pair-verify (a HomePod or Apple TV with a
/// password does; OwnTone commit e437e418).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeptPairing {
    pub(crate) keys: PairingKeys,
    pub(crate) password: Option<String>,
}

/// The password is a secret: debug output says only whether there is one.
impl std::fmt::Debug for KeptPairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeptPairing")
            .field("keys", &self.keys)
            .field("has_password", &self.password.is_some())
            .finish()
    }
}

/// The Ed25519 key an AirPlay 1 receiver that verifies the device knows bae
/// by: bae sent its public key when it paired with the PIN, and proves the
/// key at every connection. The receiver gives back no key of its own to keep
/// (pyatv keeps the seed alone, OwnTone the key pair). Each receiver gets a
/// key of its own.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct VerificationKey {
    /// The 32-byte seed of the Ed25519 key.
    pub(crate) seed: [u8; 32],
}

impl VerificationKey {
    /// A new key for one receiver, from a random seed.
    pub(crate) fn new() -> Self {
        let mut seed = [0u8; 32];
        rand::rng().fill_bytes(&mut seed);
        VerificationKey { seed }
    }

    /// The text bae keeps: the seed in lowercase hex.
    pub(crate) fn encode(&self) -> String {
        hex::encode(self.seed)
    }

    /// Read text [`Self::encode`] wrote. Any other text is malformed.
    pub(crate) fn decode(text: &str) -> Result<Self, MalformedPairingKeys> {
        Ok(VerificationKey { seed: fixed_field(text, "seed")? })
    }
}

/// The seed is a secret: debug output names the public key only.
impl std::fmt::Debug for VerificationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerificationKey")
            .field("public_key", &hex::encode(public_key(&self.seed)))
            .finish_non_exhaustive()
    }
}

/// The fields of the kept text, in order.
const FIELDS: usize = 5;

impl KeptPairing {
    /// The text bae keeps: bae's identifier, its seed, the receiver's
    /// identifier, its public key and the password, each in lowercase hex,
    /// joined by `:`. A receiver without a password has an empty last field,
    /// which no password is, since a password has a character.
    pub(crate) fn encode(&self) -> String {
        let keys = &self.keys;
        [
            hex::encode(keys.controller.id.as_bytes()),
            hex::encode(keys.controller.seed),
            hex::encode(&keys.receiver_id),
            hex::encode(keys.receiver_public),
            hex::encode(self.password.as_deref().unwrap_or_default()),
        ]
        .join(":")
    }

    /// Read text [`Self::encode`] wrote. Any other text is malformed, and
    /// says which field.
    pub(crate) fn decode(text: &str) -> Result<Self, MalformedPairingKeys> {
        let fields: Vec<&str> = text.split(':').collect();
        let [controller_id, seed, receiver_id, receiver_public, password] = fields[..] else {
            return Err(MalformedPairingKeys::FieldCount(fields.len()));
        };
        let controller_id = String::from_utf8(hex_field(controller_id, "controller id")?)
            .map_err(|error| MalformedPairingKeys::ControllerId(error.utf8_error()))?;
        let receiver_id = hex_field(receiver_id, "receiver id")?;
        if controller_id.is_empty() || receiver_id.is_empty() {
            return Err(MalformedPairingKeys::EmptyIdentifier);
        }
        let password = String::from_utf8(hex_field(password, "password")?)
            .map_err(|error| MalformedPairingKeys::Password(error.utf8_error()))?;
        Ok(KeptPairing {
            keys: PairingKeys {
                controller: ControllerIdentity {
                    id: controller_id,
                    seed: fixed_field(seed, "seed")?,
                },
                receiver_id,
                receiver_public: fixed_field(receiver_public, "receiver public key")?,
            },
            password: Some(password).filter(|password| !password.is_empty()),
        })
    }
}

fn hex_field(text: &str, field: &'static str) -> Result<Vec<u8>, MalformedPairingKeys> {
    hex::decode(text).map_err(|error| MalformedPairingKeys::Hex { field, error })
}

fn fixed_field<const N: usize>(
    text: &str,
    field: &'static str,
) -> Result<[u8; N], MalformedPairingKeys> {
    let bytes = hex_field(text, field)?;
    let len = bytes.len();
    bytes.try_into().map_err(|_bytes: Vec<u8>| MalformedPairingKeys::Length { field, len })
}

/// Kept pairing text that [`KeptPairing::encode`] didn't write.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum MalformedPairingKeys {
    #[error("the kept AirPlay pairing has {0} fields, not {FIELDS}")]
    FieldCount(usize),
    #[error("the kept AirPlay pairing's {field} is not hex: {error}")]
    Hex { field: &'static str, error: hex::FromHexError },
    #[error("the kept AirPlay pairing's {field} is {len} bytes")]
    Length { field: &'static str, len: usize },
    #[error("the kept AirPlay pairing's controller id is not UTF-8: {0}")]
    ControllerId(std::str::Utf8Error),
    #[error("the kept AirPlay pairing has an empty identifier")]
    EmptyIdentifier,
    #[error("the kept AirPlay pairing's password is not UTF-8: {0}")]
    Password(std::str::Utf8Error),
}

#[cfg(test)]
#[path = "pairing_keys_tests.rs"]
mod tests;
