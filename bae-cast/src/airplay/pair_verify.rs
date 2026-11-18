//! HomeKit pair-verify: how each AirPlay 2 session with a receiver bae paired
//! with by code proves both sides are who they paired as, and agrees on the
//! secret that session's keys come from.
//!
//! The flow pyatv's `hap_srp` (`verify1`) and OwnTone's `client_verify_*` run:
//!
//! 1. `POST /pair-verify` with `{State=M1, PublicKey=bae's X25519 ephemeral}`.
//! 2. The receiver replies `{State=M2, PublicKey=its ephemeral,
//!    EncryptedData}`; the shared secret is X25519 of the two ephemerals, and
//!    the sealed data is its identifier and its signature over its
//!    ephemeral ‖ identifier ‖ bae's ephemeral.
//! 3. `POST /pair-verify` with `{State=M3, EncryptedData}`: bae's identifier
//!    and its signature over bae's ephemeral ‖ identifier ‖ the receiver's
//!    ephemeral.
//! 4. The receiver replies `{State=M4}`. The 32-byte shared secret keys the
//!    control channel and the audio, as the SRP key of a transient pairing
//!    does.
//!
//! Like [`super::pairing`], this produces and consumes bodies only; the
//! session drives the POSTs.

use rand::RngCore;
use x25519_dalek::{x25519, X25519_BASEPOINT_BYTES};

use super::ap2_channel::hkdf32;
use super::pairing::{open, seal, sign, verify_signature, HomeKitError, PairingError};
use super::pairing_keys::PairingKeys;
use super::tlv8::{state, tlv_type, Tlv8};
use super::{check_no_error, expect_state};

/// An X25519 key, public or secret, and the shared secret, in bytes.
const X25519_LEN: usize = 32;

/// The pair-verify state machine over kept [`PairingKeys`]. Built, then driven
/// M1→M3, yielding the session's shared secret once M4 arrives.
pub(crate) struct PairVerify<'k> {
    keys: &'k PairingKeys,
    secret: [u8; X25519_LEN],
    public: [u8; X25519_LEN],
    phase: VerifyPhase,
}

enum VerifyPhase {
    /// Nothing sent yet.
    Init,
    /// M1 sent; waiting for the receiver's ephemeral and identity.
    AwaitingM2,
    /// M3 sent; waiting for the receiver to accept bae's identity.
    AwaitingM4 { shared: [u8; X25519_LEN] },
    /// M4 read; the flow is done.
    Done,
}

impl<'k> PairVerify<'k> {
    /// A pair-verify with a fresh ephemeral key, proving `keys`.
    pub(crate) fn new(keys: &'k PairingKeys) -> Self {
        let mut secret = [0u8; X25519_LEN];
        rand::rng().fill_bytes(&mut secret);
        let public = x25519(secret, X25519_BASEPOINT_BYTES);
        PairVerify { keys, secret, public, phase: VerifyPhase::Init }
    }

    /// The body of the first `POST /pair-verify` (M1): bae's ephemeral.
    pub(crate) fn start(&mut self) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, VerifyPhase::Init) {
            return Err(PairingError::WrongState);
        }
        self.phase = VerifyPhase::AwaitingM2;
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M1)
            .push(tlv_type::PUBLIC_KEY, self.public.to_vec())
            .encode())
    }

    /// Handle the receiver's M2: check it is the receiver bae paired with, and
    /// return the M3 body that proves bae is who it paired as. A receiver that
    /// answers as another identity was reset, and no longer knows bae.
    pub(crate) fn handle_m2(&mut self, response_body: &[u8]) -> Result<Vec<u8>, PairingError> {
        if !matches!(self.phase, VerifyPhase::AwaitingM2) {
            return Err(PairingError::WrongState);
        }
        self.phase = VerifyPhase::Done;
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv).map_err(no_longer_known)?;
        expect_state(&tlv, state::M2)?;

        let receiver_public =
            tlv.get(tlv_type::PUBLIC_KEY).ok_or(PairingError::MissingField("public key"))?;
        let Ok(receiver_public) = <[u8; X25519_LEN]>::try_from(receiver_public) else {
            return Err(PairingError::InvalidField("public key length"));
        };
        let shared = x25519(self.secret, receiver_public);
        // A low-order point gives an all-zero secret any party can compute.
        if shared == [0u8; X25519_LEN] {
            return Err(PairingError::InvalidField("public key"));
        }
        let key = hkdf32(b"Pair-Verify-Encrypt-Salt", &shared, b"Pair-Verify-Encrypt-Info");

        let sealed = tlv
            .get(tlv_type::ENCRYPTED_DATA)
            .ok_or(PairingError::MissingField("encrypted data"))?;
        let inner = Tlv8::decode(&open(&key, b"PV-Msg02", sealed)?)?;
        let receiver_id =
            inner.get(tlv_type::IDENTIFIER).ok_or(PairingError::MissingField("identifier"))?;
        let signature =
            inner.get(tlv_type::SIGNATURE).ok_or(PairingError::MissingField("signature"))?;
        if receiver_id != self.keys.receiver_id.as_slice() {
            return Err(PairingError::PairingLost);
        }
        verify_signature(
            &self.keys.receiver_public,
            &[&receiver_public[..], receiver_id, &self.public[..]].concat(),
            signature,
        )
        .map_err(|error| match error {
            PairingError::BadSignature => PairingError::PairingLost,
            other => other,
        })?;

        let controller = &self.keys.controller;
        let id = controller.id.as_bytes();
        let signature = sign(&controller.seed, &[&self.public[..], id, &receiver_public].concat());
        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, id.to_vec())
            .push(tlv_type::SIGNATURE, signature)
            .encode();
        self.phase = VerifyPhase::AwaitingM4 { shared };
        Ok(Tlv8::new()
            .push_u8(tlv_type::STATE, state::M3)
            .push(tlv_type::ENCRYPTED_DATA, seal(&key, b"PV-Msg03", &inner))
            .encode())
    }

    /// Handle the receiver's M4 and return the session's shared secret. A
    /// receiver that refuses bae's identity here no longer knows it.
    pub(crate) fn handle_m4(
        &mut self,
        response_body: &[u8],
    ) -> Result<[u8; X25519_LEN], PairingError> {
        let VerifyPhase::AwaitingM4 { shared } =
            std::mem::replace(&mut self.phase, VerifyPhase::Done)
        else {
            return Err(PairingError::WrongState);
        };
        let tlv = Tlv8::decode(response_body)?;
        check_no_error(&tlv).map_err(no_longer_known)?;
        expect_state(&tlv, state::M4)?;
        Ok(shared)
    }
}

/// An Authentication error in pair-verify: the receiver doesn't know the
/// identity bae proved, so it forgot this device. Every other refusal stays
/// what it is.
fn no_longer_known(error: PairingError) -> PairingError {
    match error {
        PairingError::Receiver(HomeKitError::Authentication) => PairingError::PairingLost,
        other => other,
    }
}

#[cfg(test)]
#[path = "pair_verify_tests.rs"]
mod tests;
