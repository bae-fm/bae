#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::airplay::fake_receiver::{serve_transient_pair_setup, SrpServer};
use crate::airplay::rtsp::RtspConnection;
use std::net::TcpListener;

/// The full transient handshake completes and both sides derive the same
/// shared secret, with the fake's SRP server written independently of the
/// sender.
#[test]
fn transient_handshake_agrees_on_shared_secret() {
    let mut receiver = SrpServer::new();
    let mut pairing = TransientPairing::new();

    let m1 = pairing.start().unwrap();
    let m1_tlv = Tlv8::decode(&m1).unwrap();
    assert_eq!(m1_tlv.get_u8(tlv_type::STATE), Some(state::M1));
    assert_eq!(m1_tlv.get_u8(tlv_type::METHOD), Some(0));
    assert_eq!(m1_tlv.get(tlv_type::FLAGS), Some([0x10].as_slice()));

    let m3 = pairing.handle_m2(&receiver.m2()).unwrap();
    let m4 = receiver.m4(&m3);
    let sender_secret = pairing.handle_m4(&m4).unwrap();

    assert_eq!(
        sender_secret,
        receiver.shared_key(),
        "sender and independent reference receiver derive the same SRP key"
    );
    assert_eq!(sender_secret.len(), 64, "K is SHA-512 of S");
}

/// A receiver that answers M2 with an Error TLV (e.g. it demands a PIN) is
/// surfaced as a rejection, not a panic.
#[test]
fn receiver_error_in_m2_is_surfaced() {
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let err_body = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M2)
        .push_u8(tlv_type::ERROR, 0x02) // authentication
        .encode();
    assert_eq!(
        pairing.handle_m2(&err_body),
        Err(PairingError::Receiver(HomeKitError::Authentication))
    );
}

/// Calling the steps out of order is refused.
#[test]
fn out_of_sequence_is_refused() {
    let mut pairing = TransientPairing::new();
    assert_eq!(pairing.handle_m2(&[]), Err(PairingError::WrongState));
}

/// A tampered sender M1 (as if the SRP math diverged from the reference) makes
/// the independent receiver reject the handshake — the guard that would catch
/// a real compat regression.
#[test]
#[should_panic(expected = "sender M1 must match the reference")]
fn reference_receiver_rejects_a_wrong_proof() {
    let mut receiver = SrpServer::new();
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let mut m3 = pairing.handle_m2(&receiver.m2()).unwrap();
    // Corrupt the last byte of the M3 body (inside the proof).
    *m3.last_mut().unwrap() ^= 0xFF;
    receiver.m4(&m3);
}

/// Both transient `/pair-setup` POSTs carry `X-Apple-HKP: 4`; the receiver
/// answers 501 to one that doesn't, as pyatv's fake does and shairport-sync's
/// handler reads nothing from.
#[test]
fn transient_pair_setup_sends_the_hkp_header() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        serve_transient_pair_setup(&mut reader, &mut stream)
    });

    let mut conn = RtspConnection::connect(addr.ip(), addr.port(), "bae/test").unwrap();
    let key = crate::airplay::ap2_session::run_pair_setup(&mut conn)
        .expect("the receiver accepts a pair-setup that carries X-Apple-HKP: 4");

    assert_eq!(server.join().unwrap().expect("the receiver read both POSTs"), key);
}

fn error_reply(state: u8, code: u8) -> Vec<u8> {
    Tlv8::new().push_u8(tlv_type::STATE, state).push_u8(tlv_type::ERROR, code).encode()
}

/// Every HomeKit error code the receiver can answer M2 with reaches the caller
/// as its own error, so no code is lost between the receiver and the person.
#[test]
fn each_homekit_error_code_in_m2_gives_its_own_error() {
    for (code, expected) in [
        (0x01, HomeKitError::Unknown),
        (0x02, HomeKitError::Authentication),
        (0x04, HomeKitError::MaxPeers),
        (0x05, HomeKitError::MaxTries),
        (0x06, HomeKitError::Unavailable),
        (0x07, HomeKitError::Busy),
        (0x09, HomeKitError::Other(0x09)),
        (0xFF, HomeKitError::Other(0xFF)),
    ] {
        let mut pairing = TransientPairing::new();
        pairing.start().unwrap();
        assert_eq!(
            pairing.handle_m2(&error_reply(state::M2, code)),
            Err(PairingError::Receiver(expected)),
            "error code {code}"
        );
    }
}

/// BackOff carries the retry delay the receiver states (TLV 0x08, seconds,
/// little-endian), 30 seconds here.
#[test]
fn backoff_carries_its_seconds() {
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let body = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M2)
        .push_u8(tlv_type::ERROR, 0x03)
        .push(tlv_type::RETRY_DELAY, 30u16.to_le_bytes().to_vec())
        .encode();
    assert_eq!(
        pairing.handle_m2(&body),
        Err(PairingError::Receiver(HomeKitError::BackOff { seconds: 30 }))
    );
}

/// A BackOff with no delay can't be worded as a wait, so it fails as an
/// unreadable reply and not as a wait of no time.
#[test]
fn backoff_without_a_delay_is_a_malformed_reply() {
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    assert_eq!(
        pairing.handle_m2(&error_reply(state::M2, 0x03)),
        Err(PairingError::MissingField("retry delay"))
    );
}

fn m2_with(salt: Vec<u8>, public_key: Vec<u8>) -> Vec<u8> {
    Tlv8::new()
        .push_u8(tlv_type::STATE, state::M2)
        .push(tlv_type::SALT, salt)
        .push(tlv_type::PUBLIC_KEY, public_key)
        .encode()
}

/// M2 as the pair_ap checks have it: a salt of exactly 16 bytes, a public value
/// of at most 384 bytes and not 0 mod N.
#[test]
fn a_malformed_m2_is_refused() {
    let valid_b = SrpServer::new().m2();
    let valid_b = Tlv8::decode(&valid_b).unwrap().get(tlv_type::PUBLIC_KEY).unwrap().to_vec();

    for (salt, public_key, expected) in [
        (vec![1u8; 15], valid_b.clone(), PairingError::InvalidField("salt length")),
        (vec![1u8; 17], valid_b.clone(), PairingError::InvalidField("salt length")),
        (vec![1u8; 16], vec![7u8; 385], PairingError::InvalidField("public key length")),
        (vec![1u8; 16], vec![0u8], PairingError::Srp(SrpError::InvalidServerPublic)),
    ] {
        let mut pairing = TransientPairing::new();
        pairing.start().unwrap();
        assert_eq!(pairing.handle_m2(&m2_with(salt, public_key)), Err(expected));
    }
}

/// A pairing driven to M4 against the reference receiver, with M4 replaced by
/// `m4`.
fn m4_refused(m4: impl FnOnce(Vec<u8>) -> Vec<u8>) -> PairingError {
    let mut receiver = SrpServer::new();
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let m3 = pairing.handle_m2(&receiver.m2()).unwrap();
    let reference_m4 = receiver.m4(&m3);
    pairing.handle_m4(&m4(reference_m4)).expect_err("a bad M4 is refused")
}

/// M4's server proof is required, 64 bytes, and checked.
#[test]
fn a_malformed_m4_is_refused() {
    let with_proof = |proof: Vec<u8>| {
        Tlv8::new().push_u8(tlv_type::STATE, state::M4).push(tlv_type::PROOF, proof).encode()
    };
    assert_eq!(
        m4_refused(|_| Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode()),
        PairingError::MissingField("proof")
    );
    assert_eq!(
        m4_refused(|reference| {
            let proof = Tlv8::decode(&reference).unwrap().get(tlv_type::PROOF).unwrap().to_vec();
            with_proof(proof[..63].to_vec())
        }),
        PairingError::InvalidField("proof length")
    );
    assert_eq!(
        m4_refused(|_| with_proof(vec![0u8; 64])),
        PairingError::Srp(SrpError::BadServerProof)
    );
}
