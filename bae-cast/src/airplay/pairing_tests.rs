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
    assert_eq!(pairing.handle_m2(&err_body), Err(PairingError::Rejected(0x02)));
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
