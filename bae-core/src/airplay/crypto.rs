//! RAOP audio encryption: the AES-128 session key wrapped to the receiver with
//! RSA, and the AES-128-CBC cipher applied to each audio packet.
//!
//! A legacy RAOP receiver that advertises `et=1` accepts an AES-128 session key
//! and IV the sender chooses at random. The key is RSA-OAEP(SHA-1)-encrypted to a
//! fixed public key baked into every AirPort Express, base64'd into the ANNOUNCE
//! SDP as `rsaaeskey`; the IV rides base64'd as `aesiv`. Each audio packet's
//! payload is then AES-128-CBC encrypted with that key and IV — the IV reset to
//! the session value at the *start of every packet* (CBC chaining stays within
//! one packet), and the trailing `len % 16` bytes left in the clear
//! (openairplay spec §7.2).
//!
//! The public key is Apple's, a 2048-bit modulus with exponent 65537 extracted
//! from shipping AirPort Express hardware. It is public data (documented since
//! 2004); this copy is taken from the MIT-licensed `Airstream` project. Without
//! the matching private key a sender can only *wrap* a key to the receiver, never
//! read one back — which is exactly the sender's role, so no round-trip against
//! the real key is possible (or needed).

use aes::cipher::block_padding::NoPadding;
use aes::cipher::{BlockEncryptMut, KeyIvInit};
use aes::Aes128;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rsa::{BigUint, Oaep, RsaPublicKey};
use sha1::Sha1;

/// AES-128 key/IV width in bytes.
pub const AES_BLOCK: usize = 16;

/// The Apple RAOP public-key modulus (2048-bit, big-endian hex). Exponent 65537.
const APPLE_MODULUS_HEX: &str = "\
e7d744f2a2e2788b6c1f55a08eb70544a8fa7945aa8be6c62ce5f51cbdd4dc68\
42fe3d1083dd2edec1bfd4252dc02e6f398bdf0e6148ea84855e2e442da6d626\
64f674a1f304929ade4f6893ef2df6e711a8c77a0d91c9d980822e50d12922af\
ea40ea9f0e14c0f76938c5f3882fc0323dd9fe55155f51bb5921c201629fd733\
52d5e2efaabf9ba048d7b813a2b6767f6c3ccf1eb4ce673d037b0d2ea30c5fff\
eb06f8d08adde409571a9c689fef10728855dd8cfb9a8bef5c8943ef3b5faa15\
dde698beddf3599603eb3e6f61372bb628f6559f599a78bf500687aa7f4976c0\
562d412956f8989e18a6355bd81597825e0fc875343ec782117625cdbf98447b";

/// Apple's RAOP public key, for wrapping the AES session key.
pub fn apple_public_key() -> RsaPublicKey {
    let modulus = hex::decode(APPLE_MODULUS_HEX).expect("valid Apple modulus hex");
    let n = BigUint::from_bytes_be(&modulus);
    let e = BigUint::from(65_537u32);
    RsaPublicKey::new(n, e).expect("valid Apple RAOP public key")
}

/// The RSA randomness source for OAEP. `rsa` is built against `rand_core` 0.6;
/// bae is on `rand` 0.9, so this adapter feeds the older trait from the current
/// OS RNG. (No fixed seed — OAEP's padding must be random.)
struct OaepRng;

impl rsa::rand_core::RngCore for OaepRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        use rand::RngCore as _;
        rand::rng().fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rsa::rand_core::CryptoRng for OaepRng {}

/// RSA-OAEP(SHA-1)-encrypt `aes_key` to `public_key` for the SDP `rsaaeskey`.
pub fn wrap_aes_key(public_key: &RsaPublicKey, aes_key: &[u8]) -> Result<Vec<u8>, rsa::Error> {
    public_key.encrypt(&mut OaepRng, Oaep::new::<Sha1>(), aes_key)
}

/// The AES-128-CBC cipher a RAOP session applies to each audio packet, holding
/// the session key and IV. `et=0` receivers use [`RaopCipher::none`], which
/// passes the payload through untouched.
#[derive(Clone)]
pub enum RaopCipher {
    /// No audio encryption (`et=0`).
    None,
    /// AES-128-CBC with a per-session key and IV (`et=1`).
    Aes {
        key: [u8; AES_BLOCK],
        iv: [u8; AES_BLOCK],
        /// The key already wrapped to the receiver (base64 of the RSA output),
        /// for the SDP `rsaaeskey` line.
        rsaaeskey_b64: String,
    },
}

impl RaopCipher {
    /// A pass-through cipher for an unencrypted stream.
    pub fn none() -> Self {
        RaopCipher::None
    }

    /// A fresh AES-128-CBC cipher: random key + IV, the key RSA-wrapped to
    /// `public_key`.
    pub fn new_aes(public_key: &RsaPublicKey) -> Result<Self, rsa::Error> {
        use rand::RngCore as _;
        let mut key = [0u8; AES_BLOCK];
        let mut iv = [0u8; AES_BLOCK];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut key);
        rng.fill_bytes(&mut iv);
        let wrapped = wrap_aes_key(public_key, &key)?;
        Ok(RaopCipher::Aes {
            key,
            iv,
            rsaaeskey_b64: BASE64.encode(&wrapped),
        })
    }

    /// Build an AES cipher from an explicit key and IV (the RSA wrap is computed
    /// against `public_key`). The wire path uses [`RaopCipher::new_aes`]; this
    /// keeps the key fixed for golden-fixture tests.
    #[cfg(test)]
    pub fn from_key_iv(
        public_key: &RsaPublicKey,
        key: [u8; AES_BLOCK],
        iv: [u8; AES_BLOCK],
    ) -> Result<Self, rsa::Error> {
        let wrapped = wrap_aes_key(public_key, &key)?;
        Ok(RaopCipher::Aes {
            key,
            iv,
            rsaaeskey_b64: BASE64.encode(&wrapped),
        })
    }

    /// Whether this stream is encrypted (drives whether the SDP carries the
    /// `rsaaeskey`/`aesiv` lines).
    pub fn is_encrypted(&self) -> bool {
        matches!(self, RaopCipher::Aes { .. })
    }

    /// The base64 `rsaaeskey` SDP value, present only for an encrypted stream.
    pub fn rsaaeskey_b64(&self) -> Option<&str> {
        match self {
            RaopCipher::None => None,
            RaopCipher::Aes { rsaaeskey_b64, .. } => Some(rsaaeskey_b64),
        }
    }

    /// The base64 `aesiv` SDP value, present only for an encrypted stream.
    pub fn aesiv_b64(&self) -> Option<String> {
        match self {
            RaopCipher::None => None,
            RaopCipher::Aes { iv, .. } => Some(BASE64.encode(iv)),
        }
    }

    /// Encrypt one audio packet's payload in place: whole 16-byte blocks under
    /// AES-128-CBC (IV reset to the session value for this packet), the trailing
    /// `len % 16` bytes left untouched. A no-op for an unencrypted stream.
    pub fn encrypt_packet(&self, payload: &mut [u8]) {
        let RaopCipher::Aes { key, iv, .. } = self else {
            return;
        };
        let whole = payload.len() - payload.len() % AES_BLOCK;
        if whole == 0 {
            return;
        }
        let cipher = cbc::Encryptor::<Aes128>::new(key.into(), iv.into());
        // `encrypt_padded_mut::<NoPadding>` on the whole-block prefix chains CBC
        // across exactly those blocks and never touches the plaintext tail.
        cipher
            .encrypt_padded_mut::<NoPadding>(&mut payload[..whole], whole)
            .expect("whole-block length is block-aligned");
    }
}

#[cfg(test)]
mod tests {
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
        cipher
            .decrypt_padded_mut::<NoPadding>(&mut payload[..whole])
            .expect("block-aligned");
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
        assert_eq!(
            buf[32..],
            original[32..],
            "the sub-block tail stays plaintext"
        );

        decrypt_packet(&key, &iv, &mut buf);
        assert_eq!(buf, original, "CBC decrypt recovers the payload");
    }

    /// A payload shorter than one block is passed through entirely in the clear.
    #[test]
    fn aes_cbc_short_payload_is_all_plaintext() {
        let cipher =
            RaopCipher::from_key_iv(&apple_public_key(), [0x33u8; 16], [0x44u8; 16]).unwrap();
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
        let private = RsaPrivateKey::new(&mut OaepRng, 2048).unwrap();
        let public = private.to_public_key();

        let aes_key = [0xABu8; 16];
        let wrapped = wrap_aes_key(&public, &aes_key).unwrap();
        let recovered = private.decrypt(Oaep::new::<Sha1>(), &wrapped).unwrap();
        assert_eq!(recovered, aes_key.to_vec());
    }
}
