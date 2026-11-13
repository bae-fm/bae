//! The receiver the pairing and session tests talk to: a HomeKit pair-setup
//! server that runs real SRP, written apart from [`super::srp::SrpClient`].
//!
//! It recomputes `u`, `S`, `K` and both proofs from the srptools reference
//! formulas, so a completed handshake shows the sender agrees with the
//! reference math and not only with itself. It also refuses what real
//! receivers refuse: a transient `/pair-setup` without `X-Apple-HKP: 4`
//! (pyatv's fake answers 501, shairport-sync cannot read the request).

use std::io::Write;
use std::net::TcpStream;

use num_bigint::BigUint;
use sha2::{Digest, Sha512};

use super::rtsp::read_request;
use super::srp::SrpGroup;
use super::tlv8::{state, tlv_type, Tlv8};

const USERNAME: &[u8] = b"Pair-Setup";
const TRANSIENT_PIN: &[u8] = b"3939";

/// The SRP server half of a transient pair-setup.
pub(super) struct SrpServer {
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

impl SrpServer {
    pub(super) fn new() -> Self {
        let group = SrpGroup::rfc5054_3072();
        let salt = vec![0xA5u8; 16];
        // x = H(s | H(I | ":" | P)); v = g^x.
        let inner = h(&[USERNAME, b":", TRANSIENT_PIN]);
        let x = h_int(&[BigUint::from_bytes_be(&salt).to_bytes_be().as_slice(), inner.as_slice()]);
        let v = group.g.modpow(&x, &group.n);
        // k = H(N | PAD(g)); B = (k*v + g^b) mod N.
        let width = group.n.to_bytes_be().len();
        let k = h_int(&[group.n.to_bytes_be().as_slice(), pad(width, &group.g).as_slice()]);
        let b = BigUint::from_bytes_be(&[0x7Cu8; 32]);
        let b_pub = (&k * &v + group.g.modpow(&b, &group.n)) % &group.n;
        SrpServer { group, salt, b, b_pub, v, session: None }
    }

    /// The M2 body: salt + B.
    pub(super) fn m2(&self) -> Vec<u8> {
        Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push(tlv_type::SALT, self.salt.clone())
            .push(tlv_type::PUBLIC_KEY, self.b_pub.to_bytes_be())
            .encode()
    }

    /// Verify the sender's M3 (A + M1) and produce M4 (server proof M2).
    pub(super) fn m4(&mut self, m3_body: &[u8]) -> Vec<u8> {
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
            h(&[USERNAME]).as_slice(),
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

    pub(super) fn shared_key(&self) -> &[u8] {
        &self.session.as_ref().unwrap().0
    }
}

/// Write one RTSP reply with `status_line` (`200 OK`) and `body`.
pub(super) fn respond(writer: &mut TcpStream, status_line: &str, cseq: &str, body: &[u8]) {
    let mut out =
        format!("RTSP/1.0 {status_line}\r\nCSeq: {cseq}\r\nContent-Length: {}\r\n\r\n", body.len())
            .into_bytes();
    out.extend_from_slice(body);
    writer.write_all(&out).unwrap();
    writer.flush().unwrap();
}

/// Serve a transient pair-setup (M1 to M4) on `reader`/`writer` the way
/// shairport-sync does: each POST must carry `X-Apple-HKP: 4`, else the
/// receiver answers 501 and the pairing fails here. Returns the SRP key both
/// sides now hold.
pub(super) fn serve_transient_pair_setup(
    reader: &mut impl std::io::BufRead,
    writer: &mut TcpStream,
) -> Result<Vec<u8>, String> {
    let mut srp = SrpServer::new();
    for step in ["M1", "M3"] {
        let request = read_request(reader)
            .map_err(|e| format!("reading {step}: {e}"))?
            .ok_or_else(|| format!("connection closed before {step}"))?;
        let cseq = request.cseq().to_string();
        if request.header("X-Apple-HKP") != Some("4") {
            respond(writer, "501 Not Implemented", &cseq, &[]);
            return Err(format!("{step} without X-Apple-HKP: 4"));
        }
        let reply = if step == "M1" { srp.m2() } else { srp.m4(&request.body) };
        respond(writer, "200 OK", &cseq, &reply);
    }
    Ok(srp.shared_key().to_vec())
}
