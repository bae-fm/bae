use super::*;
use chacha20poly1305::Nonce;

/// The audio cipher round-trips: a packet sealed by the sender decrypts under
/// the same key, nonce, and associated data, and the counter advances.
#[test]
fn audio_cipher_round_trips_and_advances_the_counter() {
    let shared = [0x5Au8; 32];
    let mut cipher = Ap2AudioCipher::from_shared_secret(&shared);
    assert_eq!(cipher.counter(), 0);

    let aad = [0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
    let payload = vec![9u8; 40];
    let sealed = cipher.seal(&aad, &payload);
    assert_eq!(cipher.counter(), 1);

    // The last 8 bytes are the nonce counter (0 for the first packet).
    let (ct, nonce_tail) = sealed.split_at(sealed.len() - 8);
    assert_eq!(nonce_tail, &0u64.to_le_bytes());
    assert_eq!(ct.len(), payload.len() + 16, "payload + Poly1305 tag");

    // Decrypt independently to prove the seal.
    let key = crate::airplay::ap2_channel::audio_key(&shared);
    let dec = ChaCha20Poly1305::new(Key::from_slice(&key));
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..].copy_from_slice(nonce_tail);
    let plain =
        dec.decrypt(Nonce::from_slice(&nonce_bytes), Payload { msg: ct, aad: &aad }).unwrap();
    assert_eq!(plain, payload);
}

/// PTP is chosen only when the receiver advertises the PTP feature bit.
#[test]
fn timing_protocol_follows_features() {
    assert_eq!(TimingProtocol::from_features(0), TimingProtocol::Ntp);
    assert_eq!(TimingProtocol::from_features(1 << 42), TimingProtocol::Ptp);
}
