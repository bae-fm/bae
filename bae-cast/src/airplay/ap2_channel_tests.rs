use super::*;

/// A message sealed by the sender is read back by a peer that swaps write/read
/// keys — the HomeKit-transport role reversal — proving the framing, nonce
/// counters, and length-as-AAD all agree.
#[test]
fn seal_reassembles_across_the_role_reversal() {
    let shared = [0x42u8; 32];
    let (write, read) = control_keys(&shared);
    let mut sender = HapChannel::from_keys(&write, &read);
    // The receiver's write key is the sender's read key, and vice versa.
    let mut receiver = HapChannel::from_keys(&read, &write);

    let msg = b"SETUP rtsp://x/1 RTSP/1.0\r\nCSeq: 1\r\n\r\n";
    let sealed = sender.seal(msg);
    // The length prefix is the plaintext length, and there's a 16-byte tag.
    assert_eq!(u16::from_le_bytes([sealed[0], sealed[1]]) as usize, msg.len());
    assert_eq!(sealed.len(), 2 + msg.len() + 16);

    receiver.feed(&sealed);
    let recovered = receiver.next_block().unwrap().unwrap();
    assert_eq!(recovered, msg);
    assert!(receiver.next_block().unwrap().is_none());
}

/// Counters advance per block, so successive messages use distinct nonces and
/// still decrypt in order.
#[test]
fn counters_advance_per_block() {
    let (write, read) = control_keys(&[1u8; 32]);
    let mut sender = HapChannel::from_keys(&write, &read);
    let mut receiver = HapChannel::from_keys(&read, &write);

    let a = sender.seal(b"first");
    let b = sender.seal(b"second");
    // Deliver out of one buffer to prove reassembly, in order.
    receiver.feed(&a);
    receiver.feed(&b);
    assert_eq!(receiver.next_block().unwrap().unwrap(), b"first");
    assert_eq!(receiver.next_block().unwrap().unwrap(), b"second");
}

/// A partial frame yields nothing until the rest arrives.
#[test]
fn partial_frame_waits_for_more() {
    let (write, read) = control_keys(&[7u8; 32]);
    let mut sender = HapChannel::from_keys(&write, &read);
    let mut receiver = HapChannel::from_keys(&read, &write);
    let sealed = sender.seal(b"hello world");

    receiver.feed(&sealed[..5]);
    assert!(receiver.next_block().unwrap().is_none());
    receiver.feed(&sealed[5..]);
    assert_eq!(receiver.next_block().unwrap().unwrap(), b"hello world");
}

/// The HKDF primitive matches RFC 5869's published Test Case 1 (SHA-256), so
/// the derivation the control/audio keys ride on is validated against the
/// standard, not only against itself.
#[test]
fn hkdf_matches_rfc5869_test_case_1() {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let ikm = [0x0bu8; 22];
    let salt: Vec<u8> = (0x00u8..=0x0c).collect();
    let info: Vec<u8> = (0xf0u8..=0xf9).collect();

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut okm = [0u8; 42];
    hk.expand(&info, &mut okm).unwrap();

    let expected = hex::decode(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
         34007208d5b887185865",
    )
    .unwrap();
    assert_eq!(okm.as_slice(), expected.as_slice());
}

/// The audio key and control keys are distinct expansions of one secret.
#[test]
fn derived_keys_are_distinct() {
    let shared = [0x9Au8; 32];
    let (write, read) = control_keys(&shared);
    let shk = audio_key(&shared);
    assert_ne!(write, read);
    assert_ne!(write, shk);
    assert_ne!(read, shk);
}
