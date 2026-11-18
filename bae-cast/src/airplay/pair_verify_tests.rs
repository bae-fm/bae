use super::*;
use crate::airplay::fake_receiver::{
    error_reply, PairedController, ReceiverIdentity, VerifyServer,
};
use crate::airplay::pairing::public_key;
use crate::airplay::pairing_keys::ControllerIdentity;

fn keys_for(receiver: &ReceiverIdentity) -> PairingKeys {
    PairingKeys {
        controller: ControllerIdentity { id: "controller".to_string(), seed: [0x11; 32] },
        receiver_id: receiver.id.clone(),
        receiver_public: receiver.public(),
    }
}

fn known(keys: &PairingKeys) -> Vec<PairedController> {
    vec![PairedController {
        id: keys.controller.id.clone().into_bytes(),
        public: public_key(&keys.controller.seed),
    }]
}

/// Both sides of a pair-verify derive the same secret, with the receiver's
/// side written apart from the sender's.
#[test]
fn both_sides_agree_on_the_secret() {
    let receiver = ReceiverIdentity::new();
    let keys = keys_for(&receiver);
    let mut server = VerifyServer::new(receiver);
    let mut verify = PairVerify::new(&keys);

    let m2 = server.m2(&verify.start().unwrap());
    let m3 = verify.handle_m2(&m2).unwrap();
    let (m4, server_secret) = server.m4(&m3, &known(&keys));
    let secret = verify.handle_m4(&m4).unwrap();

    assert_eq!(Some(secret), server_secret);
}

/// A receiver whose key isn't the kept one was reset: the pairing is lost.
#[test]
fn a_receiver_with_another_key_lost_the_pairing() {
    let receiver = ReceiverIdentity::new();
    let keys = keys_for(&receiver);
    let mut server = VerifyServer::new(receiver.after_reset());
    let mut verify = PairVerify::new(&keys);

    let m2 = server.m2(&verify.start().unwrap());
    assert!(matches!(verify.handle_m2(&m2), Err(PairingError::PairingLost)));
}

/// Authentication at M2 or M4 is the pairing lost; another error is itself.
#[test]
fn refusals_read_as_what_they_say() {
    let receiver = ReceiverIdentity::new();
    let keys = keys_for(&receiver);

    let mut verify = PairVerify::new(&keys);
    verify.start().unwrap();
    assert!(matches!(
        verify.handle_m2(&error_reply(state::M2, 0x02)),
        Err(PairingError::PairingLost)
    ));

    let mut verify = PairVerify::new(&keys);
    verify.start().unwrap();
    assert!(matches!(
        verify.handle_m2(&error_reply(state::M2, 0x07)),
        Err(PairingError::Receiver(HomeKitError::Busy))
    ));

    let mut server = VerifyServer::new(receiver);
    let mut verify = PairVerify::new(&keys);
    let m2 = server.m2(&verify.start().unwrap());
    verify.handle_m2(&m2).unwrap();
    assert!(matches!(
        verify.handle_m4(&error_reply(state::M4, 0x02)),
        Err(PairingError::PairingLost)
    ));
}

/// A receiver ephemeral of a low-order point gives a secret anyone knows,
/// which bae refuses.
#[test]
fn a_low_order_ephemeral_is_refused() {
    let keys = keys_for(&ReceiverIdentity::new());
    let mut verify = PairVerify::new(&keys);
    verify.start().unwrap();
    let m2 = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M2)
        .push(tlv_type::PUBLIC_KEY, vec![0u8; 32])
        .push(tlv_type::ENCRYPTED_DATA, vec![0u8; 32])
        .encode();
    assert!(matches!(verify.handle_m2(&m2), Err(PairingError::InvalidField("public key"))));
}

/// Calling the steps out of order is refused.
#[test]
fn out_of_sequence_is_refused() {
    let keys = keys_for(&ReceiverIdentity::new());
    let mut verify = PairVerify::new(&keys);
    assert!(matches!(verify.handle_m2(&[]), Err(PairingError::WrongState)));
    assert!(matches!(verify.handle_m4(&[]), Err(PairingError::WrongState)));
}
