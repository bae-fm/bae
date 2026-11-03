use super::*;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use x25519_dalek::{PublicKey, StaticSecret};

/// A scripted fake AirPlay 2 receiver running the pair-verify *server* side
/// independently of [`PairVerify`]: it generates its own ephemeral X25519 key
/// and Ed25519 identity, derives the same session key from first principles,
/// and checks the sender's M3 signature — so a completed exchange is evidence
/// the sender interoperates with the reference math, not with itself.
struct FakeReceiver {
    ephemeral: StaticSecret,
    ephemeral_public: PublicKey,
    signing: SigningKey,
    shared: Option<[u8; 32]>,
}

impl FakeReceiver {
    fn new() -> Self {
        let ephemeral = StaticSecret::random_from_rng(SecureRng);
        let ephemeral_public = PublicKey::from(&ephemeral);
        FakeReceiver {
            ephemeral,
            ephemeral_public,
            signing: SigningKey::generate(&mut SecureRng),
            shared: None,
        }
    }

    /// Given the sender's M1 (its ephemeral key), build M2.
    fn m2(&mut self, m1: &[u8]) -> Vec<u8> {
        let tlv = Tlv8::decode(m1).unwrap();
        assert_eq!(tlv.get_u8(tlv_type::STATE), Some(state::M1));
        let sender_pub: [u8; 32] = tlv.get(tlv_type::PUBLIC_KEY).unwrap().try_into().unwrap();

        let shared = self.ephemeral.diffie_hellman(&PublicKey::from(sender_pub)).to_bytes();
        self.shared = Some(shared);
        let session_key = hkdf32(b"Pair-Verify-Encrypt-Salt", &shared, b"Pair-Verify-Encrypt-Info");
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));

        // Sign device_ephemeral || sender_ephemeral.
        let mut signed = Vec::new();
        signed.extend_from_slice(self.ephemeral_public.as_bytes());
        signed.extend_from_slice(&sender_pub);
        let signature = self.signing.sign(&signed);
        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, b"fake-receiver".to_vec())
            .push(tlv_type::SIGNATURE, signature.to_bytes().to_vec())
            .encode();
        let sealed = cipher
            .encrypt(&pair_verify_nonce(b"PV-Msg02"), Payload { msg: &inner, aad: &[] })
            .unwrap();

        Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push(tlv_type::PUBLIC_KEY, self.ephemeral_public.as_bytes().to_vec())
            .push(tlv_type::ENCRYPTED_DATA, sealed)
            .encode()
    }

    /// Verify the sender's M3 signature and return M4.
    fn m4(&self, m3: &[u8], sender_verifying: &VerifyingKey, sender_pub: &[u8; 32]) -> Vec<u8> {
        let tlv = Tlv8::decode(m3).unwrap();
        assert_eq!(tlv.get_u8(tlv_type::STATE), Some(state::M3));
        let encrypted = tlv.get(tlv_type::ENCRYPTED_DATA).unwrap();

        let session_key = hkdf32(
            b"Pair-Verify-Encrypt-Salt",
            self.shared.as_ref().unwrap(),
            b"Pair-Verify-Encrypt-Info",
        );
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));
        let inner = cipher
            .decrypt(&pair_verify_nonce(b"PV-Msg03"), encrypted)
            .expect("M3 decrypts with the shared session key");
        let inner_tlv = Tlv8::decode(&inner).unwrap();
        let sig_bytes: [u8; 64] = inner_tlv.get(tlv_type::SIGNATURE).unwrap().try_into().unwrap();

        let mut signed = Vec::new();
        signed.extend_from_slice(sender_pub);
        signed.extend_from_slice(self.ephemeral_public.as_bytes());
        sender_verifying
            .verify(&signed, &Signature::from_bytes(&sig_bytes))
            .expect("sender M3 signature verifies against its ephemeral identity");

        Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode()
    }
}

/// The full pair-verify completes and both sides derive the same X25519
/// shared secret, the fake receiver running the server independently.
#[test]
fn pair_verify_agrees_on_shared_secret() {
    let mut receiver = FakeReceiver::new();
    let mut verify = PairVerify::new();

    let m1 = verify.start().unwrap();
    let sender_pub = *verify.ephemeral_public.as_bytes();
    let sender_verifying = verify.signing.verifying_key();

    let m2 = receiver.m2(&m1);
    let m3 = verify.handle_m2(&m2).unwrap();
    let m4 = receiver.m4(&m3, &sender_verifying, &sender_pub);
    let sender_secret = verify.handle_m4(&m4).unwrap();

    assert_eq!(
        sender_secret,
        receiver.shared.unwrap(),
        "sender and independent receiver derive the same X25519 secret"
    );
}

/// An error TLV in M2 surfaces as a rejection, not a panic.
#[test]
fn receiver_error_is_surfaced() {
    let mut verify = PairVerify::new();
    verify.start().unwrap();
    let err =
        Tlv8::new().push_u8(tlv_type::STATE, state::M2).push_u8(tlv_type::ERROR, 0x02).encode();
    assert_eq!(verify.handle_m2(&err), Err(PairingError::Rejected(0x02)));
}

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
