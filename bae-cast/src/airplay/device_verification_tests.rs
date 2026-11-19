use super::*;
use crate::airplay::fake_verification_receiver::{listen_for_verification, VerificationReceiver};
use crate::airplay::rtsp::SenderIdentity;

const CLIENT: &str = "Studio Mac";

fn connect(port: u16) -> RtspConnection {
    RtspConnection::connect([127, 0, 0, 1].into(), port, SenderIdentity::new("bae/test", CLIENT))
        .expect("the fake listens")
}

/// The PIN start names this device, the pairing with the PIN shown gives the
/// receiver bae's key, and a later connection verifies with it.
#[test]
fn a_device_that_paired_with_the_pin_verifies_on_the_next_connection() {
    let (port, receiver) = listen_for_verification(VerificationReceiver::new(&["0482"]));
    let key = VerificationKey::new();
    {
        let mut conn = connect(port);
        show_pin(&mut conn).expect("the receiver shows a PIN");
        pair_with_pin(&mut conn, "0482", &key).expect("the receiver took the PIN");
    }
    verify(&mut connect(port), &key).expect("the receiver knows the key");

    let receiver = receiver.lock().unwrap();
    assert_eq!(receiver.shown_beside.as_deref(), Some(CLIENT));
    assert_eq!(receiver.paired, [public_key(&key.seed)]);
}

/// A PIN other than the one the receiver shows is a wrong PIN, and the
/// receiver keeps no key.
#[test]
fn a_wrong_pin_reads_as_a_wrong_pin() {
    let (port, receiver) = listen_for_verification(VerificationReceiver::new(&["0482"]));
    let mut conn = connect(port);
    show_pin(&mut conn).unwrap();
    let paired = pair_with_pin(&mut conn, "1234", &VerificationKey::new());
    assert!(matches!(paired, Err(VerificationError::WrongPin)), "{paired:?}");
    drop(conn);
    assert!(receiver.lock().unwrap().paired.is_empty());
}

/// A receiver that refuses either step of pair-verify no longer knows the
/// key bae kept.
#[test]
fn a_refused_verification_reads_as_a_forgotten_key() {
    let (port, receiver) = listen_for_verification(VerificationReceiver::new(&[]));
    let unknown = verify(&mut connect(port), &VerificationKey::new());
    assert!(matches!(unknown, Err(VerificationError::NoLongerKnown)), "{unknown:?}");

    receiver.lock().unwrap().refuse_verify_keys = Some("470 Connection Authorization Required");
    let refused = verify(&mut connect(port), &VerificationKey::new());
    assert!(matches!(refused, Err(VerificationError::NoLongerKnown)), "{refused:?}");
}

/// A pair-verify reply too short to hold the receiver's key and proof is a
/// reply that doesn't check out, not a refusal.
#[test]
fn a_short_verification_reply_is_a_bad_reply() {
    let verify = PairVerify::new(&VerificationKey { seed: [0x11; 32] });
    let answered = verify.answer(&[0x09; 40]);
    assert!(
        matches!(answered, Err(VerificationError::BadReply { field: "proof", .. })),
        "{answered:?}"
    );
}
