//! The pairing a receiver that asked for a code and bae agreed on: the
//! identities each later session's pair-verify proves, and the text bae keeps
//! them as.
//!
//! bae-cast owns this text's format. Whoever keeps it stores the string by
//! name and never reads inside it.

use super::pairing::{public_key, ED25519_PUBLIC_KEY_LEN};

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

/// The fields of the kept text, in order.
const FIELDS: usize = 4;

impl PairingKeys {
    /// The text bae keeps: bae's identifier, its seed, the receiver's
    /// identifier and its public key, each in lowercase hex, joined by `:`.
    pub(crate) fn encode(&self) -> String {
        [
            hex::encode(self.controller.id.as_bytes()),
            hex::encode(self.controller.seed),
            hex::encode(&self.receiver_id),
            hex::encode(self.receiver_public),
        ]
        .join(":")
    }

    /// Read text [`Self::encode`] wrote. Any other text is malformed, and
    /// says which field.
    pub(crate) fn decode(text: &str) -> Result<Self, MalformedPairingKeys> {
        let fields: Vec<&str> = text.split(':').collect();
        let [controller_id, seed, receiver_id, receiver_public] = fields[..] else {
            return Err(MalformedPairingKeys::FieldCount(fields.len()));
        };
        let controller_id = String::from_utf8(hex_field(controller_id, "controller id")?)
            .map_err(|error| MalformedPairingKeys::ControllerId(error.utf8_error()))?;
        let receiver_id = hex_field(receiver_id, "receiver id")?;
        if controller_id.is_empty() || receiver_id.is_empty() {
            return Err(MalformedPairingKeys::EmptyIdentifier);
        }
        Ok(PairingKeys {
            controller: ControllerIdentity { id: controller_id, seed: fixed_field(seed, "seed")? },
            receiver_id,
            receiver_public: fixed_field(receiver_public, "receiver public key")?,
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

/// Kept pairing text that [`PairingKeys::encode`] didn't write.
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
}

#[cfg(test)]
#[path = "pairing_keys_tests.rs"]
mod tests;
