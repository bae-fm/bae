use super::*;
use num_bigint::BigUint;
use sha2::{Digest, Sha512};

/// A scripted fake receiver running the SRP *server* side, implemented
/// independently of [`SrpClient`] — it recomputes `u`, `S`, `K`, and the
/// proofs from the srptools reference formulas here rather than calling any
/// sender helper, so a matching handshake is evidence the sender interoperates
/// with the reference math, not with itself.
struct FakeReceiver {
    group: SrpGroup,
    salt: Vec<u8>,
    b: BigUint,
    b_pub: BigUint,
    v: BigUint,
    /// Captured once M3 arrives, to build M4.
    session: Option<(Vec<u8>, Vec<u8>, BigUint)>, // (K, M1, A)
}

fn h(parts: &[&[u8]]) -> Vec<u8> {
    let mut d = Sha512::new();
    for p in parts {
        d.update(p);
    }
    d.finalize().to_vec()
}

fn h_int(parts: &[&[u8]]) -> BigUint {
    BigUint::from_bytes_be(&h(parts))
}

fn pad(width: usize, v: &BigUint) -> Vec<u8> {
    let b = v.to_bytes_be();
    let mut out = vec![0u8; width.saturating_sub(b.len())];
    out.extend_from_slice(&b);
    out
}

impl FakeReceiver {
    fn new() -> Self {
        let group = SrpGroup::rfc5054_3072();
        let salt = vec![0xA5u8; 16];
        // x = H(s | H(I | ":" | P)); v = g^x.
        let inner = h(&[PAIR_SETUP_USERNAME, b":", TRANSIENT_PIN]);
        let x = h_int(&[BigUint::from_bytes_be(&salt).to_bytes_be().as_slice(), inner.as_slice()]);
        let v = group.g.modpow(&x, &group.n);
        // k = H(N | PAD(g)); B = (k*v + g^b) mod N.
        let width = group.n.to_bytes_be().len();
        let k = h_int(&[group.n.to_bytes_be().as_slice(), pad(width, &group.g).as_slice()]);
        let b = BigUint::from_bytes_be(&[0x7Cu8; 32]);
        let b_pub = (&k * &v + group.g.modpow(&b, &group.n)) % &group.n;
        FakeReceiver { group, salt, b, b_pub, v, session: None }
    }

    /// The M2 body: salt + B.
    fn m2(&self) -> Vec<u8> {
        Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push(tlv_type::SALT, self.salt.clone())
            .push(tlv_type::PUBLIC_KEY, self.b_pub.to_bytes_be())
            .encode()
    }

    /// Verify the sender's M3 (A + M1) and produce M4 (server proof M2).
    fn m4(&mut self, m3_body: &[u8]) -> Vec<u8> {
        let tlv = Tlv8::decode(m3_body).unwrap();
        assert_eq!(tlv.get_u8(tlv_type::STATE), Some(state::M3));
        let a_bytes = tlv.get(tlv_type::PUBLIC_KEY).unwrap().to_vec();
        let client_m1 = tlv.get(tlv_type::PROOF).unwrap().to_vec();
        let a_pub = BigUint::from_bytes_be(&a_bytes);

        let n = &self.group.n;
        let width = n.to_bytes_be().len();
        // u = H(PAD(A) | PAD(B)); S = (A * v^u)^b mod N; K = H(S).
        let u = h_int(&[pad(width, &a_pub).as_slice(), pad(width, &self.b_pub).as_slice()]);
        let s = (&a_pub * self.v.modpow(&u, n) % n).modpow(&self.b, n);
        let key = h(&[s.to_bytes_be().as_slice()]);

        // M1 = H(H(N) XOR H(g) | H(I) | s | A | B | K); reject a mismatch.
        let h_n = h_int(&[n.to_bytes_be().as_slice()]);
        let h_g = h_int(&[self.group.g.to_bytes_be().as_slice()]);
        let expected_m1 = h(&[
            (h_n ^ h_g).to_bytes_be().as_slice(),
            h(&[PAIR_SETUP_USERNAME]).as_slice(),
            BigUint::from_bytes_be(&self.salt).to_bytes_be().as_slice(),
            a_bytes.as_slice(),
            self.b_pub.to_bytes_be().as_slice(),
            key.as_slice(),
        ]);
        assert_eq!(client_m1, expected_m1, "sender M1 must match the reference");

        // M2 = H(A | M1 | K).
        let m2 = h(&[a_bytes.as_slice(), client_m1.as_slice(), key.as_slice()]);
        self.session = Some((key, client_m1, a_pub));
        Tlv8::new().push_u8(tlv_type::STATE, state::M4).push(tlv_type::PROOF, m2).encode()
    }

    fn shared_key(&self) -> &[u8] {
        &self.session.as_ref().unwrap().0
    }
}

/// The full transient handshake completes and both sides derive the same
/// shared secret, with the fake's SRP server written independently of the
/// sender.
#[test]
fn transient_handshake_agrees_on_shared_secret() {
    let mut receiver = FakeReceiver::new();
    let mut pairing = TransientPairing::new();

    let m1 = pairing.start().unwrap();
    let m1_tlv = Tlv8::decode(&m1).unwrap();
    assert_eq!(m1_tlv.get_u8(tlv_type::STATE), Some(state::M1));
    assert_eq!(m1_tlv.get_u8(tlv_type::METHOD), Some(0));
    assert_eq!(m1_tlv.get(tlv_type::FLAGS), Some([0x10].as_slice()));

    let m3 = pairing.handle_m2(&receiver.m2()).unwrap();
    let m4 = receiver.m4(&m3);
    let sender_secret = pairing.handle_m4(&m4).unwrap();

    assert_eq!(
        sender_secret,
        receiver.shared_key(),
        "sender and independent reference receiver derive the same SRP key"
    );
    assert_eq!(sender_secret.len(), 64, "K is SHA-512 of S");
}

/// A receiver that answers M2 with an Error TLV (e.g. it demands a PIN) is
/// surfaced as a rejection, not a panic.
#[test]
fn receiver_error_in_m2_is_surfaced() {
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let err_body = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M2)
        .push_u8(tlv_type::ERROR, 0x02) // authentication
        .encode();
    assert_eq!(pairing.handle_m2(&err_body), Err(PairingError::Rejected(0x02)));
}

/// Calling the steps out of order is refused.
#[test]
fn out_of_sequence_is_refused() {
    let mut pairing = TransientPairing::new();
    assert_eq!(pairing.handle_m2(&[]), Err(PairingError::WrongState));
}

/// A tampered sender M1 (as if the SRP math diverged from the reference) makes
/// the independent receiver reject the handshake — the guard that would catch
/// a real compat regression.
#[test]
#[should_panic(expected = "sender M1 must match the reference")]
fn reference_receiver_rejects_a_wrong_proof() {
    let mut receiver = FakeReceiver::new();
    let mut pairing = TransientPairing::new();
    pairing.start().unwrap();
    let mut m3 = pairing.handle_m2(&receiver.m2()).unwrap();
    // Corrupt the last byte of the M3 body (inside the proof).
    *m3.last_mut().unwrap() ^= 0xFF;
    receiver.m4(&m3);
}
