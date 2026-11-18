//! The pairing POSTs over an AirPlay 2 receiver's plaintext control
//! connection: transient pair-setup, the PIN start, pair-setup with a code,
//! and pair-verify. Each drives its state machine
//! ([`super::pairing`], [`super::pair_verify`]) one request at a time.

use super::ap2_session::Ap2Error;
use super::pair_verify::PairVerify;
use super::pairing::{CodePairing, PairingError, TransientPairing, CODE_HKP, TRANSIENT_HKP};
use super::pairing_keys::{ControllerIdentity, PairingKeys};
use super::rtsp::{Method, RtspConnection, RtspRequest};
use super::tlv8::{tlv_type, Tlv8};

/// The `step` of the rejection of the first `/pair-setup` POST: a receiver
/// that answers a transient one with 470 wants a PIN.
pub(crate) const PAIR_SETUP_M1: &str = "/pair-setup M1";
const PAIR_SETUP_M3: &str = "/pair-setup M3";
const PAIR_SETUP_M5: &str = "/pair-setup M5";
const PAIR_PIN_START: &str = "/pair-pin-start";
const PAIR_VERIFY_M1: &str = "/pair-verify M1";
const PAIR_VERIFY_M3: &str = "/pair-verify M3";

/// Drive transient pair-setup (SRP) over the plaintext RTSP socket, returning
/// the SRP key. Every request after M4 is encrypted with keys derived from it,
/// and the audio key comes from the same secret.
pub(crate) fn run_transient_pair_setup(conn: &mut RtspConnection) -> Result<Vec<u8>, Ap2Error> {
    let mut pairing = TransientPairing::new();
    let m1 = pairing.start()?;
    let m2 = pair_post(conn, "/pair-setup", PAIR_SETUP_M1, TRANSIENT_HKP, m1)?;
    let m3 = pairing.handle_m2(&m2)?;
    let m4 = pair_post(conn, "/pair-setup", PAIR_SETUP_M3, TRANSIENT_HKP, m3)?;
    Ok(pairing.handle_m4(&m4)?)
}

/// Ask the receiver to show a PIN (`POST /pair-pin-start`, no body), for a
/// pair-setup with that PIN on the same connection.
pub(crate) fn start_pin(conn: &mut RtspConnection) -> Result<(), Ap2Error> {
    let response = conn.request(
        &RtspRequest::new(Method::Post, "/pair-pin-start").header("X-Apple-HKP", CODE_HKP),
    )?;
    if response.is_success() {
        Ok(())
    } else {
        Err(Ap2Error::Rejected { step: PAIR_PIN_START, status: response.status })
    }
}

/// Drive pair-setup with `code` (M1 to M6), introducing bae as `identity`,
/// and return the keys every later session pair-verifies with.
pub(crate) fn run_code_pair_setup(
    conn: &mut RtspConnection,
    code: &str,
    identity: ControllerIdentity,
) -> Result<PairingKeys, Ap2Error> {
    let mut pairing = CodePairing::new(code, identity);
    let m1 = pairing.start()?;
    let m2 = pair_post(conn, "/pair-setup", PAIR_SETUP_M1, CODE_HKP, m1)?;
    let m3 = pairing.handle_m2(&m2)?;
    let m4 = pair_post(conn, "/pair-setup", PAIR_SETUP_M3, CODE_HKP, m3)?;
    let m5 = pairing.handle_m4(&m4)?;
    let m6 = pair_post(conn, "/pair-setup", PAIR_SETUP_M5, CODE_HKP, m5)?;
    Ok(pairing.handle_m6(&m6)?)
}

/// Drive pair-verify with `keys` (M1 to M4) and return the session's shared
/// secret. A receiver that answers M1 with 470 asks for a pairing, so it no
/// longer knows this device.
pub(crate) fn run_pair_verify(
    conn: &mut RtspConnection,
    keys: &PairingKeys,
) -> Result<[u8; 32], Ap2Error> {
    let mut verify = PairVerify::new(keys);
    let m1 = verify.start()?;
    let m2 = match pair_post(conn, "/pair-verify", PAIR_VERIFY_M1, CODE_HKP, m1) {
        Err(Ap2Error::Rejected { status: 470, .. }) => {
            return Err(PairingError::PairingLost.into());
        }
        replied => replied?,
    };
    let m3 = verify.handle_m2(&m2)?;
    let m4 = pair_post(conn, "/pair-verify", PAIR_VERIFY_M3, CODE_HKP, m3)?;
    Ok(verify.handle_m4(&m4)?)
}

/// POST a pairing body to `path`, as the step named `step`, marked with the
/// `X-Apple-HKP` value receivers require, and return the response body for
/// the state machine to read. A receiver may answer a refusal with a non-2xx
/// status and an Error TLV (shairport-sync does), so a body that carries an
/// error goes to the state machine whatever the status, which reads it as the
/// HomeKit error it names; a non-2xx with no error item is the status, naming
/// the POST.
fn pair_post(
    conn: &mut RtspConnection,
    path: &str,
    step: &'static str,
    hkp: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, Ap2Error> {
    let response = conn.request(
        &RtspRequest::with_body(Method::Post, path, "application/octet-stream", body)
            .header("X-Apple-HKP", hkp),
    )?;
    let names_an_error = Tlv8::decode(&response.body)
        .is_ok_and(|tlv| tlv.get_u8(tlv_type::ERROR).is_some_and(|code| code != 0));
    if response.is_success() || names_an_error {
        Ok(response.body)
    } else {
        Err(Ap2Error::Rejected { step, status: response.status })
    }
}

#[cfg(test)]
#[path = "ap2_pairing_tests.rs"]
mod tests;
