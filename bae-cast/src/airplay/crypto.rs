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
//! 2004); this copy is the modulus of the keypair in the MIT-licensed `Airstream`
//! project (`qasim/Airstream`, `master`, `Airstream/Airport.h`), extracted with
//! openssl. Without the matching private key a sender can only *wrap* a key to
//! the receiver, never read one back — which is exactly the sender's role, so no
//! round-trip against the real key is possible (or needed). Crates: rsa 0.9,
//! aes 0.8, cbc 0.1.

use aes::cipher::block_padding::NoPadding;
use aes::cipher::{BlockEncryptMut, KeyIvInit};
use aes::Aes128;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rsa::{BigUint, Oaep, RsaPublicKey};
use sha1::Sha1;

/// AES-128 key/IV width in bytes.
pub(crate) const AES_BLOCK: usize = 16;

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
pub(crate) fn apple_public_key() -> RsaPublicKey {
    let modulus = hex::decode(APPLE_MODULUS_HEX).expect("valid Apple modulus hex");
    let n = BigUint::from_bytes_be(&modulus);
    let e = BigUint::from(65_537u32);
    RsaPublicKey::new(n, e).expect("valid Apple RAOP public key")
}

/// RSA-OAEP(SHA-1)-encrypt `aes_key` to `public_key` for the SDP `rsaaeskey`.
pub(crate) fn wrap_aes_key(
    public_key: &RsaPublicKey,
    aes_key: &[u8],
) -> Result<Vec<u8>, rsa::Error> {
    public_key.encrypt(&mut super::secure_rng::SecureRng, Oaep::new::<Sha1>(), aes_key)
}

/// The AES-128-CBC cipher a RAOP session applies to each audio packet, holding
/// the session key and IV. `et=0` receivers use [`RaopCipher::none`], which
/// passes the payload through untouched.
#[derive(Clone)]
pub(crate) enum RaopCipher {
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
    pub(crate) fn none() -> Self {
        RaopCipher::None
    }

    /// A fresh AES-128-CBC cipher: random key + IV, the key RSA-wrapped to
    /// `public_key`.
    pub(crate) fn new_aes(public_key: &RsaPublicKey) -> Result<Self, rsa::Error> {
        use rand::RngCore as _;
        let mut key = [0u8; AES_BLOCK];
        let mut iv = [0u8; AES_BLOCK];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut key);
        rng.fill_bytes(&mut iv);
        let wrapped = wrap_aes_key(public_key, &key)?;
        Ok(RaopCipher::Aes { key, iv, rsaaeskey_b64: BASE64.encode(&wrapped) })
    }

    /// Build an AES cipher from an explicit key and IV (the RSA wrap is computed
    /// against `public_key`). The wire path uses [`RaopCipher::new_aes`]; this
    /// keeps the key fixed for golden-fixture tests.
    #[cfg(test)]
    pub(crate) fn from_key_iv(
        public_key: &RsaPublicKey,
        key: [u8; AES_BLOCK],
        iv: [u8; AES_BLOCK],
    ) -> Result<Self, rsa::Error> {
        let wrapped = wrap_aes_key(public_key, &key)?;
        Ok(RaopCipher::Aes { key, iv, rsaaeskey_b64: BASE64.encode(&wrapped) })
    }

    /// Whether this stream is encrypted.
    #[cfg(test)]
    pub(crate) fn is_encrypted(&self) -> bool {
        matches!(self, RaopCipher::Aes { .. })
    }

    /// The base64 `rsaaeskey` SDP value, present only for an encrypted stream.
    pub(crate) fn rsaaeskey_b64(&self) -> Option<&str> {
        match self {
            RaopCipher::None => None,
            RaopCipher::Aes { rsaaeskey_b64, .. } => Some(rsaaeskey_b64),
        }
    }

    /// The base64 `aesiv` SDP value, present only for an encrypted stream.
    pub(crate) fn aesiv_b64(&self) -> Option<String> {
        match self {
            RaopCipher::None => None,
            RaopCipher::Aes { iv, .. } => Some(BASE64.encode(iv)),
        }
    }

    /// Encrypt one audio packet's payload in place: whole 16-byte blocks under
    /// AES-128-CBC (IV reset to the session value for this packet), the trailing
    /// `len % 16` bytes left untouched. A no-op for an unencrypted stream.
    pub(crate) fn encrypt_packet(&self, payload: &mut [u8]) {
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
#[path = "crypto_tests.rs"]
mod tests;
