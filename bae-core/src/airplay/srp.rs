//! SRP-6a, the password-authenticated key exchange HomeKit pairing runs on.
//!
//! The math is written directly over bignums to the semantics a real receiver
//! accepts, established by pyatv's (MIT, `master` @ 2024) use of the `srptools`
//! library (BSD) — which pairs real HomePods — and cross-checked against the
//! published RFC 5054 Appendix B test vector (see the tests). The RustCrypto
//! `srp` crate (0.6, the current release, whose source was read) is deliberately
//! *not* used: its `M1` proof is `H(A|B|K)` (its own source notes this is not the
//! spec) and it does not pad `A`/`B` in `u`, so a real receiver rejects its
//! proof. The reference semantics here are:
//!
//! - `k = H(N | PAD(g))`
//! - `x = H(s | H(I | ":" | P))`
//! - `A = g^a mod N`, `B = k·v + g^b mod N`, `v = g^x mod N`
//! - `u = H(PAD(A) | PAD(B))`
//! - client `S = (B − k·v)^(a + u·x) mod N`
//! - `K = H(S)`
//! - `M1 = H(H(N) XOR H(g) | H(I) | s | A | B | K)`
//! - `M2 = H(A | M1 | K)`
//!
//! Integers fed into a hash are minimal big-endian (leading zeros stripped),
//! except where `PAD(...)` widens them to `N`'s byte length — matching
//! `srptools`' `int_to_bytes`/`pad`.

use num_bigint::BigUint;
use sha2::{Digest, Sha512};

/// The SRP group: modulus `N` and generator `g`.
pub struct SrpGroup {
    pub n: BigUint,
    pub g: BigUint,
}

impl SrpGroup {
    /// The 3072-bit group (RFC 5054 group 3072 / RFC 3526 MODP), which HomeKit
    /// pairing uses.
    pub fn rfc5054_3072() -> Self {
        SrpGroup {
            n: BigUint::parse_bytes(RFC5054_N_3072, 16).expect("valid 3072-bit modulus"),
            g: BigUint::from(5u8),
        }
    }
}

/// A hash of `data`. Boxed as a function so the tests can drive the SRP math with
/// SHA-1 (the RFC 5054 vector's hash) while production uses SHA-512.
pub type HashFn = fn(&[u8]) -> Vec<u8>;

/// SHA-512, the hash HomeKit SRP uses.
pub fn sha512(data: &[u8]) -> Vec<u8> {
    Sha512::digest(data).to_vec()
}

/// A client-side SRP-6a exchange in progress.
pub struct SrpClient {
    group: SrpGroup,
    hash: HashFn,
    username: Vec<u8>,
    password: Vec<u8>,
    a: BigUint,
    a_pub: BigUint,
    /// Set by [`SrpClient::process`].
    completed: Option<Completed>,
}

struct Completed {
    session_key: Vec<u8>,
    m1: Vec<u8>,
    /// Kept to check the receiver's `M2 = H(A | M1 | K)`.
    a_pub_bytes: Vec<u8>,
}

/// An SRP failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrpError {
    /// The receiver's public value `B` was 0 mod N — an abort per RFC 5054.
    InvalidServerPublic,
    /// `process` has not been called yet.
    NotProcessed,
    /// The receiver's `M2` proof did not verify.
    BadServerProof,
}

impl SrpClient {
    /// Start an exchange with a chosen private `a` (production supplies random
    /// bytes; the vector test supplies the fixed `a`).
    pub fn new(
        group: SrpGroup,
        hash: HashFn,
        username: &[u8],
        password: &[u8],
        a: BigUint,
    ) -> Self {
        let a_pub = group.g.modpow(&a, &group.n);
        SrpClient {
            group,
            hash,
            username: username.to_vec(),
            password: password.to_vec(),
            a,
            a_pub,
            completed: None,
        }
    }

    /// The client public value `A`, minimal big-endian — what the sender puts on
    /// the wire.
    pub fn public_a(&self) -> Vec<u8> {
        self.a_pub.to_bytes_be()
    }

    /// Process the receiver's `salt` and `B`: derive `x`, `u`, `S`, `K`, and the
    /// client proof `M1`.
    pub fn process(&mut self, salt: &[u8], b_pub_bytes: &[u8]) -> Result<(), SrpError> {
        let n = &self.group.n;
        let b_pub = BigUint::from_bytes_be(b_pub_bytes);
        if &b_pub % n == BigUint::from(0u8) {
            return Err(SrpError::InvalidServerPublic);
        }

        let width = n.to_bytes_be().len();
        // Salt as an integer, minimal big-endian (srptools treats it as int).
        let salt_min = BigUint::from_bytes_be(salt).to_bytes_be();

        // x = H(s | H(I | ":" | P))
        let mut inner = self.username.clone();
        inner.push(b':');
        inner.extend_from_slice(&self.password);
        let inner_hash = (self.hash)(&inner);
        let x = self.hash_to_int(&[salt_min.as_slice(), inner_hash.as_slice()]);

        // k = H(N | PAD(g))
        let k = self.hash_to_int(&[
            n.to_bytes_be().as_slice(),
            pad(width, &self.group.g).as_slice(),
        ]);
        // v = g^x mod N
        let v = self.group.g.modpow(&x, n);
        // u = H(PAD(A) | PAD(B))
        let u = self.hash_to_int(&[
            pad(width, &self.a_pub).as_slice(),
            pad(width, &b_pub).as_slice(),
        ]);

        // S = (B - k*v)^(a + u*x) mod N, with the subtraction taken mod N so the
        // base is non-negative for modpow.
        let kv = (&k * &v) % n;
        let base = (&b_pub % n + n - kv) % n;
        let exp = &self.a + &u * &x;
        let s = base.modpow(&exp, n);

        // K = H(S)
        let session_key = (self.hash)(&s.to_bytes_be());

        // M1 = H(H(N) XOR H(g) | H(I) | s | A | B | K)
        let h_n = BigUint::from_bytes_be(&(self.hash)(&n.to_bytes_be()));
        let h_g = BigUint::from_bytes_be(&(self.hash)(&self.group.g.to_bytes_be()));
        let h_n_xor_h_g = (h_n ^ h_g).to_bytes_be();
        let h_i = (self.hash)(&self.username);
        let a_pub_bytes = self.a_pub.to_bytes_be();
        let b_pub_min = b_pub.to_bytes_be();
        let m1 = (self.hash)(
            &[
                h_n_xor_h_g.as_slice(),
                h_i.as_slice(),
                salt_min.as_slice(),
                a_pub_bytes.as_slice(),
                b_pub_min.as_slice(),
                session_key.as_slice(),
            ]
            .concat(),
        );

        self.completed = Some(Completed {
            session_key,
            m1,
            a_pub_bytes,
        });
        Ok(())
    }

    /// The client proof `M1` to send to the receiver.
    pub fn proof_m1(&self) -> Result<&[u8], SrpError> {
        self.completed
            .as_ref()
            .map(|c| c.m1.as_slice())
            .ok_or(SrpError::NotProcessed)
    }

    /// The shared session key `K = H(S)` — the secret the audio-stream keys are
    /// derived from.
    pub fn shared_key(&self) -> Result<&[u8], SrpError> {
        self.completed
            .as_ref()
            .map(|c| c.session_key.as_slice())
            .ok_or(SrpError::NotProcessed)
    }

    /// Verify the receiver's proof `M2 = H(A | M1 | K)`.
    pub fn verify_m2(&self, server_m2: &[u8]) -> Result<(), SrpError> {
        let c = self.completed.as_ref().ok_or(SrpError::NotProcessed)?;
        let expected = (self.hash)(
            &[
                c.a_pub_bytes.as_slice(),
                c.m1.as_slice(),
                c.session_key.as_slice(),
            ]
            .concat(),
        );
        if expected == server_m2 {
            Ok(())
        } else {
            Err(SrpError::BadServerProof)
        }
    }

    fn hash_to_int(&self, parts: &[&[u8]]) -> BigUint {
        BigUint::from_bytes_be(&(self.hash)(&parts.concat()))
    }
}

/// `val` as big-endian, left-padded with zeros to `width` bytes.
fn pad(width: usize, val: &BigUint) -> Vec<u8> {
    let bytes = val.to_bytes_be();
    if bytes.len() >= width {
        bytes
    } else {
        let mut out = vec![0u8; width - bytes.len()];
        out.extend_from_slice(&bytes);
        out
    }
}

/// The RFC 5054 group-3072 modulus, hex.
const RFC5054_N_3072: &[u8] = b"FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74\
020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374\
FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE\
386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF0598D\
A48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB9ED5\
29077096966D670C354E4ABC9804F1746C08CA18217C32905E462E36CE3BE39E7\
72C180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF6955817183995497\
CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D04507A33A85521AB\
DF1CBA64ECFB850458DBEF0A8AEA71575D060C7DB3970F85A6E1E4C7ABF5AE8CD\
B0933D71E8C94E04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864D87602733E\
C86A64521F2B18177B200CBBE117577A615D6C770988C0BAD946E208E24FA074E\
5AB3143DB5BFCE0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF";

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::Sha1;

    fn sha1(data: &[u8]) -> Vec<u8> {
        Sha1::digest(data).to_vec()
    }

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn big(s: &str) -> BigUint {
        BigUint::from_bytes_be(&hex(s))
    }

    /// RFC 5054 Appendix B test vector (SHA-1, 1024-bit group, I="alice",
    /// P="password123"). The published `S` (premaster secret) validates the
    /// primitives k/x/u and the modexp end to end, independent of any fake this
    /// crate also authors. `A` matches because the vector's `a` is fed in.
    #[test]
    fn matches_rfc5054_appendix_b() {
        let n = big(
            "EEAF0AB9 ADB38DD6 9C33F80A FA8FC5E8 60726187 75FF3C0B 9EA2314C \
             9C256576 D674DF74 96EA81D3 383B4813 D692C6E0 E0D5D8E2 50B98BE4 \
             8E495C1D 6089DAD1 5DC7D7B4 6154D6B6 CE8EF4AD 69B15D49 82559B29 \
             7BCF1885 C529F566 660E57EC 68EDBC3C 05726CC0 2FD4CBF4 976EAA9A \
             FD5138FE 8376435B 9FC61D2F C0EB06E3",
        );
        let group = SrpGroup {
            n,
            g: BigUint::from(2u8),
        };
        let a = big("60975527 035CF2AD 1989806F 0407210B C81EDC04 E2762A56 AFD529DD DA2D4393");
        let salt = hex("BEB25379 D1A8581E B5A72767 3A2441EE");
        let b_pub = hex(
            "BD0C6151 2C692C0C B6D041FA 01BB152D 4916A1E7 7AF46AE1 05393011 \
             BAF38964 DC46A067 0DD125B9 5A981652 236F99D9 B681CBF8 7837EC99 \
             6C6DA044 53728610 D0C6DDB5 8B318885 D7D82C7F 8DEB75CE 7BD4FBAA \
             37089E6F 9C6059F3 88838E7A 00030B33 1EB76840 910440B1 B27AAEAE \
             EB4012B7 D7665238 A8E3FB00 4B117B58",
        );

        let mut client = SrpClient::new(group, sha1, b"alice", b"password123", a);

        // A matches the published value.
        assert_eq!(
            client.public_a(),
            hex(
                "61D5E490 F6F1B795 47B0704C 436F523D D0E560F0 C64115BB 72557EC4 \
                 4352E890 3211C046 92272D8B 2D1A5358 A2CF1B6E 0BFCF99F 921530EC \
                 8E393561 79EAE45E 42BA92AE ACED8251 71E1E8B9 AF6D9C03 E1327F44 \
                 BE087EF0 6530E69F 66615261 EEF54073 CA11CF58 58F0EDFD FE15EFEA \
                 B349EF5D 76988A36 72FAC47B 0769447B"
            )
        );

        client.process(&salt, &b_pub).unwrap();

        // The shared key is H(S); check against H(published S) computed here with
        // an independent one-liner, so the derivation isn't taken on faith.
        let published_s = hex(
            "B0DC82BA BCF30674 AE450C02 87745E79 90A3381F 63B387AA F271A10D \
             233861E3 59B48220 F7C4693C 9AE12B0A 6F67809F 0876E2D0 13800D6C \
             41BB59B6 D5979B5C 00A172B4 A2A5903A 0BDCAF8A 709585EB 2AFAFA8F \
             3499B200 210DCC1F 10EB3394 3CD67FC8 8A2F39A4 BE5BEC4E C0A3212D \
             C346D7E4 74B29EDE 8A469FFE CA686E5A",
        );
        assert_eq!(
            client.shared_key().unwrap(),
            sha1(&published_s).as_slice(),
            "K = H(S) must derive from the RFC 5054 premaster secret"
        );
    }

    /// The 3072-bit HomeKit group parses and A is in range.
    #[test]
    fn homekit_group_produces_a_public_value() {
        let a = BigUint::from_bytes_be(&[0x42u8; 32]);
        let client = SrpClient::new(SrpGroup::rfc5054_3072(), sha512, b"Pair-Setup", b"3939", a);
        let a_pub = client.public_a();
        // A is a 3072-bit value: at most 384 bytes, and not zero.
        assert!(!a_pub.is_empty() && a_pub.len() <= 384);
    }

    #[test]
    fn pad_left_fills_to_width() {
        assert_eq!(pad(4, &BigUint::from(0x01u8)), vec![0, 0, 0, 1]);
        assert_eq!(pad(2, &BigUint::from(0x0102u16)), vec![1, 2]);
        // Already at or over width: unchanged.
        assert_eq!(pad(1, &BigUint::from(0x0102u16)), vec![1, 2]);
    }
}
