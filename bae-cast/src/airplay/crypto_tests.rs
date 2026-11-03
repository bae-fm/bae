use super::*;

/// Decrypt a RAOP packet payload — the receiver's side, used only to check the
/// sender's CBC round-trips.
fn decrypt_packet(key: &[u8; 16], iv: &[u8; 16], payload: &mut [u8]) {
    use aes::cipher::block_padding::NoPadding;
    use aes::cipher::BlockDecryptMut as _;
    let whole = payload.len() - payload.len() % AES_BLOCK;
    if whole == 0 {
        return;
    }
    let cipher = cbc::Decryptor::<Aes128>::new(key.into(), iv.into());
    cipher.decrypt_padded_mut::<NoPadding>(&mut payload[..whole]).expect("block-aligned");
}

#[test]
fn apple_key_is_2048_bits() {
    use rsa::traits::PublicKeyParts as _;
    let key = apple_public_key();
    assert_eq!(key.size(), 256, "2048-bit modulus");
}

/// The pass-through cipher never mutates the payload and carries no SDP keys.
#[test]
fn unencrypted_is_pass_through() {
    let cipher = RaopCipher::none();
    assert!(!cipher.is_encrypted());
    assert!(cipher.rsaaeskey_b64().is_none());
    let mut payload = vec![1u8, 2, 3, 4, 5];
    cipher.encrypt_packet(&mut payload);
    assert_eq!(payload, vec![1, 2, 3, 4, 5]);
}

/// AES-CBC encrypt→decrypt returns the original payload, and the trailing
/// sub-block bytes are left in the clear both ways.
#[test]
fn aes_cbc_round_trips_with_plaintext_tail() {
    let key = [0x11u8; 16];
    let iv = [0x22u8; 16];
    let cipher = RaopCipher::from_key_iv(&apple_public_key(), key, iv).unwrap();

    // 40 bytes = two whole blocks (32) + an 8-byte tail left plaintext.
    let original: Vec<u8> = (0..40u8).collect();
    let mut buf = original.clone();
    cipher.encrypt_packet(&mut buf);

    assert_ne!(buf[..32], original[..32], "whole blocks are encrypted");
    assert_eq!(buf[32..], original[32..], "the sub-block tail stays plaintext");

    decrypt_packet(&key, &iv, &mut buf);
    assert_eq!(buf, original, "CBC decrypt recovers the payload");
}

/// A payload shorter than one block is passed through entirely in the clear.
#[test]
fn aes_cbc_short_payload_is_all_plaintext() {
    let cipher = RaopCipher::from_key_iv(&apple_public_key(), [0x33u8; 16], [0x44u8; 16]).unwrap();
    let mut buf = vec![9u8; 15];
    cipher.encrypt_packet(&mut buf);
    assert_eq!(buf, vec![9u8; 15]);
}

/// The encrypted cipher exposes the SDP `rsaaeskey`/`aesiv` values; the IV
/// base64 round-trips to the IV bytes.
#[test]
fn encrypted_cipher_exposes_sdp_keys() {
    let iv = [0x55u8; 16];
    let cipher = RaopCipher::from_key_iv(&apple_public_key(), [0x66u8; 16], iv).unwrap();
    assert!(cipher.is_encrypted());
    assert!(cipher.rsaaeskey_b64().is_some());
    let iv_b64 = cipher.aesiv_b64().unwrap();
    assert_eq!(BASE64.decode(iv_b64).unwrap(), iv.to_vec());
}

/// RSA-OAEP wrap → unwrap round-trips against a locally generated keypair
/// (Apple's private key is unavailable, so this proves the wrap primitive
/// independently of the fixed public key).
#[test]
fn rsa_oaep_wrap_unwraps_with_matching_private_key() {
    use rsa::RsaPrivateKey;
    let private = RsaPrivateKey::new(&mut crate::airplay::secure_rng::SecureRng, 2048).unwrap();
    let public = private.to_public_key();

    let aes_key = [0xABu8; 16];
    let wrapped = wrap_aes_key(&public, &aes_key).unwrap();
    let recovered = private.decrypt(Oaep::new::<Sha1>(), &wrapped).unwrap();
    assert_eq!(recovered, aes_key.to_vec());
}
