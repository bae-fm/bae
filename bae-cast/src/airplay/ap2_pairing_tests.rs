use super::*;
use crate::airplay::fake_receiver::{
    CodeReceiver, CodeRefusal, PairedController, ReceiverCode, Served,
};
use crate::airplay::pairing::{public_key, HomeKitError};
use std::net::TcpListener;

/// Serve one connection with `receiver` while `sender` drives a connection
/// to it, which closes once `sender` returns. Gives back what `sender`
/// returned, the receiver, and how the connection ended.
fn with_receiver<T>(
    mut receiver: CodeReceiver,
    sender: impl FnOnce(&mut RtspConnection) -> T,
) -> (T, CodeReceiver, Served) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let served = receiver.serve(&mut reader, &mut stream);
            (receiver, served)
        });
        let mut conn = connect(port);
        let sent = sender(&mut conn);
        drop(conn);
        let (receiver, served) = server.join().unwrap();
        (sent, receiver, served)
    })
}

/// The name bae gives this device in each pairing request.
const NAME: &str = "Kitchen Mac";

fn connect(port: u16) -> RtspConnection {
    RtspConnection::connect("127.0.0.1".parse().unwrap(), port, "bae/test").unwrap()
}

fn identity() -> ControllerIdentity {
    ControllerIdentity { id: "6F1C2A4E-0B3D-4C5E-8F70-9A1B2C3D4E5F".to_string(), seed: [0x11; 32] }
}

/// Pair-setup with the PIN the receiver shows, after a PIN start on the same
/// connection: both sides end up holding each other's identity.
#[test]
fn pin_start_then_pair_setup_with_the_right_pin_pairs() {
    let (keys, receiver, served) =
        with_receiver(CodeReceiver::new(ReceiverCode::pins(&["4821"])), |conn| {
            start_pin(conn, NAME).unwrap();
            run_code_pair_setup(conn, NAME, "4821", identity()).unwrap()
        });
    assert_eq!(served, Served::Closed);
    assert_eq!(receiver.requests, ["/pair-pin-start", "/pair-setup", "/pair-setup", "/pair-setup"]);
    assert_eq!(receiver.pin_shown_beside(), Some(NAME));
    assert_eq!(keys.controller, identity());
    assert_eq!(keys.receiver_id, receiver.identity.id);
    assert_eq!(keys.receiver_public, receiver.identity.public());
    assert_eq!(
        receiver.paired,
        [PairedController { id: identity().id.into_bytes(), public: public_key(&identity().seed) }]
    );
}

/// A wrong PIN is its own failure; the receiver shows a new PIN on the next
/// PIN start, and pairing with that one, leading zero kept, succeeds on the
/// same connection.
#[test]
fn a_wrong_pin_fails_and_the_next_pin_pairs() {
    let (keys, receiver, _) =
        with_receiver(CodeReceiver::new(ReceiverCode::pins(&["4821", "0937"])), |conn| {
            start_pin(conn, NAME).unwrap();
            let wrong = run_code_pair_setup(conn, NAME, "4812", identity()).unwrap_err();
            assert!(
                matches!(
                    wrong,
                    Ap2Error::Pairing(PairingError::WrongCode { retry_after_seconds: None })
                ),
                "{wrong:?}"
            );
            start_pin(conn, NAME).unwrap();
            run_code_pair_setup(conn, NAME, "0937", identity()).unwrap()
        });
    assert_eq!(keys.receiver_public, receiver.identity.public());
    assert_eq!(receiver.paired.len(), 1);
}

/// A wrong PIN whose refusal states a retry delay, as HomePod OS 27 answers,
/// reports the delay.
#[test]
fn a_wrong_pin_reports_the_receiver_s_retry_delay() {
    let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["4821"]));
    receiver.retry_delay = Some(4);
    let (wrong, _, _) = with_receiver(receiver, |conn| {
        start_pin(conn, NAME).unwrap();
        run_code_pair_setup(conn, NAME, "4812", identity()).unwrap_err()
    });
    assert!(
        matches!(
            wrong,
            Ap2Error::Pairing(PairingError::WrongCode { retry_after_seconds: Some(4) })
        ),
        "{wrong:?}"
    );
}

/// A password pairs with no PIN start.
#[test]
fn a_password_pairs_with_no_pin_start() {
    let password = "kitchen speaker pässword";
    let (_, receiver, _) =
        with_receiver(CodeReceiver::new(ReceiverCode::Password(password.to_string())), |conn| {
            run_code_pair_setup(conn, NAME, password, identity()).unwrap()
        });
    assert_eq!(receiver.requests, ["/pair-setup", "/pair-setup", "/pair-setup"]);
    assert_eq!(receiver.paired.len(), 1);
}

/// Each HomeKit error a receiver answers a code pair-setup's M1 with reaches
/// the caller as that error.
#[test]
fn a_refusal_of_a_code_pair_setup_is_the_receiver_s_error() {
    for (code, expected) in [
        (0x05, HomeKitError::MaxTries),
        (0x07, HomeKitError::Busy),
        (0x06, HomeKitError::Unavailable),
        (0x04, HomeKitError::MaxPeers),
    ] {
        let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["4821"]));
        receiver.refusal = Some(CodeRefusal::SetupM2(code));
        let (refused, _, _) = with_receiver(receiver, |conn| {
            start_pin(conn, NAME).unwrap();
            run_code_pair_setup(conn, NAME, "4821", identity()).unwrap_err()
        });
        assert!(
            matches!(refused, Ap2Error::Pairing(PairingError::Receiver(error)) if error == expected),
            "code {code}: {refused:?}"
        );
    }
}

/// Pair on one connection, then pair-verify on another with the keys that
/// pairing gave: both sides derive the same session secret.
fn paired_receiver() -> (CodeReceiver, PairingKeys) {
    let (keys, receiver, _) =
        with_receiver(CodeReceiver::new(ReceiverCode::pins(&["4821"])), |conn| {
            start_pin(conn, NAME).unwrap();
            run_code_pair_setup(conn, NAME, "4821", identity()).unwrap()
        });
    (receiver, keys)
}

#[test]
fn pair_verify_with_kept_keys_agrees_on_the_session_secret() {
    let (receiver, keys) = paired_receiver();
    let (secret, receiver, served) =
        with_receiver(receiver, |conn| run_pair_verify(conn, NAME, &keys).unwrap());
    assert_eq!(served, Served::Verified(secret));
    assert_eq!(receiver.requests.last().map(String::as_str), Some("/pair-verify"));
}

/// A change to a receiver that makes it forget bae.
type Forget = fn(&mut CodeReceiver);

/// The ways a receiver says it no longer knows bae: Authentication at M2 or
/// at M4 (it forgot the controller), 470 to M1, or answering as another key
/// (it was reset). Each is the pairing lost.
#[test]
fn a_receiver_that_forgot_this_device_reads_as_the_pairing_lost() {
    let cases: [(&str, Forget); 4] = [
        ("Authentication at M4", |receiver| receiver.paired.clear()),
        ("Authentication at M2", |receiver| {
            receiver.refusal = Some(CodeRefusal::VerifyM2(0x02));
        }),
        ("470 to M1", |receiver| receiver.refusal = Some(CodeRefusal::VerifyM1With470)),
        ("a new key", |receiver| receiver.identity = receiver.identity.after_reset()),
    ];
    for (case, change) in cases {
        let (mut receiver, keys) = paired_receiver();
        change(&mut receiver);
        let (lost, _, _) =
            with_receiver(receiver, |conn| run_pair_verify(conn, NAME, &keys).unwrap_err());
        assert!(matches!(lost, Ap2Error::Pairing(PairingError::PairingLost)), "{case}: {lost:?}");
    }
}

/// Any other refusal of pair-verify stays what the receiver said.
#[test]
fn another_refusal_of_pair_verify_is_the_receiver_s_error() {
    let (mut receiver, keys) = paired_receiver();
    receiver.refusal = Some(CodeRefusal::VerifyM2(0x07));
    let (refused, _, _) =
        with_receiver(receiver, |conn| run_pair_verify(conn, NAME, &keys).unwrap_err());
    assert!(
        matches!(refused, Ap2Error::Pairing(PairingError::Receiver(HomeKitError::Busy))),
        "{refused:?}"
    );
}

/// A receiver with an identifier other than the kept one is another
/// receiver: the pairing is lost.
#[test]
fn another_receiver_identifier_is_the_pairing_lost() {
    let (mut receiver, keys) = paired_receiver();
    receiver.identity = receiver.identity.with_id(b"58:55:CA:00:00:01");
    let (lost, _, _) =
        with_receiver(receiver, |conn| run_pair_verify(conn, NAME, &keys).unwrap_err());
    assert!(matches!(lost, Ap2Error::Pairing(PairingError::PairingLost)), "{lost:?}");
}
