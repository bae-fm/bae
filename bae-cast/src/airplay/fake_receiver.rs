//! The receiver the pairing and session tests talk to: a HomeKit pair-setup
//! server that runs real SRP, written apart from [`super::srp::SrpClient`],
//! and a pair-verify server written apart from [`super::pair_verify`].
//!
//! It recomputes `u`, `S`, `K` and both proofs from the srptools reference
//! formulas, so a completed handshake shows the sender agrees with the
//! reference math and not only with itself. Its pairing with a code and its
//! pair-verify make their own HKDF, Ed25519, X25519 and ChaCha20-Poly1305
//! calls with the labels typed out again, so a sender that agrees with it
//! agrees with HAP and not with its own helpers. It also refuses what real
//! receivers refuse: a transient `/pair-setup` without `X-Apple-HKP: 4`
//! (pyatv's fake answers 501, shairport-sync cannot read the request).

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use std::io::Write;
use std::net::TcpStream;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use num_bigint::BigUint;
use sha2::{Digest, Sha512};

use super::rtsp::{read_request, ParsedRequest};
use super::srp::SrpGroup;
use super::tlv8::{state, tlv_type, Tlv8};

const USERNAME: &[u8] = b"Pair-Setup";
const TRANSIENT_PIN: &[u8] = b"3939";
/// HomeKit's Authentication error code.
const AUTHENTICATION: u8 = 0x02;

/// The SRP server half of a pair-setup.
pub(crate) struct SrpServer {
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
    /// The server of a transient pair-setup.
    pub(crate) fn new() -> Self {
        Self::with_password(TRANSIENT_PIN)
    }

    /// The server of a pair-setup over `password`, a PIN's digits or a
    /// password.
    pub(crate) fn with_password(password: &[u8]) -> Self {
        let group = SrpGroup::rfc5054_3072();
        let salt = vec![0xA5u8; 16];
        // x = H(s | H(I | ":" | P)); v = g^x.
        let inner = h(&[USERNAME, b":", password]);
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
    pub(crate) fn m2(&self) -> Vec<u8> {
        Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push(tlv_type::SALT, self.salt.clone())
            .push(tlv_type::PUBLIC_KEY, self.b_pub.to_bytes_be())
            .encode()
    }

    /// Verify the sender's M3 (A + M1) and produce M4 (server proof M2).
    pub(crate) fn m4(&mut self, m3_body: &[u8]) -> Vec<u8> {
        self.answer_m3(m3_body).expect("sender M1 must match the reference")
    }

    /// The M4 that answers the sender's M3: the server proof, or `None` when
    /// the sender's proof isn't the one its password gives.
    fn answer_m3(&mut self, m3_body: &[u8]) -> Option<Vec<u8>> {
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
        if client_m1 != expected_m1 {
            return None;
        }

        // M2 = H(A | M1 | K).
        let m2 = h(&[a_bytes.as_slice(), client_m1.as_slice(), key.as_slice()]);
        self.session = Some((key, client_m1, a_pub));
        Some(Tlv8::new().push_u8(tlv_type::STATE, state::M4).push(tlv_type::PROOF, m2).encode())
    }

    pub(crate) fn shared_key(&self) -> &[u8] {
        &self.session.as_ref().unwrap().0
    }
}

/// The `User-Agent`s HomePod and Apple TV on OS 27 answered with 200 in
/// OwnTone issue 2042 and its fix; they answered 403 to every other one tried.
const ACCEPTED_USER_AGENTS: [&str; 3] = ["AirPlay/380.20.1", "AirPlay/999.0.0", "iTunes/12"];

/// Answer `request` with 403, as HomePod and tvOS 27 do, unless its
/// `User-Agent` is an AirPlay sender's. `step` names the request in the error.
fn refuse_unless_airplay_sender(
    request: &ParsedRequest,
    writer: &mut TcpStream,
    step: &str,
) -> Result<(), String> {
    let user_agent = request.header("User-Agent");
    if user_agent.is_some_and(|agent| ACCEPTED_USER_AGENTS.contains(&agent)) {
        return Ok(());
    }
    respond(writer, "403 Forbidden", request.cseq(), &[]);
    Err(format!("{step} with User-Agent {user_agent:?}"))
}

/// Write one RTSP reply with `status_line` (`200 OK`) and `body`.
pub(crate) fn respond(writer: &mut TcpStream, status_line: &str, cseq: &str, body: &[u8]) {
    let mut out =
        format!("RTSP/1.0 {status_line}\r\nCSeq: {cseq}\r\nContent-Length: {}\r\n\r\n", body.len())
            .into_bytes();
    out.extend_from_slice(body);
    writer.write_all(&out).unwrap();
    writer.flush().unwrap();
}

/// Serve a transient pair-setup (M1 to M4) on `reader`/`writer` the way
/// shairport-sync does: each POST must carry `X-Apple-HKP: 4`, else the
/// receiver answers 501 and the pairing fails here. Each must also name the
/// sender in `X-Apple-Client-Name`, as tvOS 27 needs. Returns the SRP key both
/// sides now hold.
pub(crate) fn serve_transient_pair_setup(
    reader: &mut impl std::io::BufRead,
    writer: &mut TcpStream,
) -> Result<Vec<u8>, String> {
    let mut srp = SrpServer::new();
    for step in ["M1", "M3"] {
        let request = read_request(reader)
            .map_err(|e| format!("reading {step}: {e}"))?
            .ok_or_else(|| format!("connection closed before {step}"))?;
        let cseq = request.cseq().to_string();
        refuse_unless_airplay_sender(&request, writer, step)?;
        if request.header("X-Apple-HKP") != Some("4") {
            respond(writer, "501 Not Implemented", &cseq, &[]);
            return Err(format!("{step} without X-Apple-HKP: 4"));
        }
        if request.header("X-Apple-Client-Name").is_none_or(str::is_empty) {
            respond(writer, "400 Bad Request", &cseq, &[]);
            return Err(format!("{step} without X-Apple-Client-Name"));
        }
        let reply = if step == "M1" { srp.m2() } else { srp.m4(&request.body) };
        respond(writer, "200 OK", &cseq, &reply);
    }
    Ok(srp.shared_key().to_vec())
}

/// What a scripted receiver does when a sender pairs with it.
pub(crate) enum PairSetupScript {
    /// Run the whole transient pair-setup, then hold the connection until the
    /// sender closes it.
    Complete,
    /// Answer M1 with HTTP 470, as receivers that want a PIN do.
    RefuseM1With470,
    /// Answer M1 with HTTP 200 and this Error TLV code.
    ErrorInM2(u8),
    /// Answer M1 with M2, then close the connection when M3 arrives.
    DropAfterM2,
}

/// Listen on a loopback port and run `script` for the first connection, on
/// its own thread. Returns the port.
pub(crate) fn listen_for_pair_setup(script: PairSetupScript) -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind a loopback port");
    let port = listener.local_addr().expect("read the bound port").port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the sender connects");
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        match script {
            PairSetupScript::Complete => {
                serve_transient_pair_setup(&mut reader, &mut stream)
                    .expect("the sender pairs the way a receiver reads it");
                let mut rest = Vec::new();
                std::io::Read::read_to_end(&mut reader, &mut rest).ok();
            }
            PairSetupScript::RefuseM1With470 => {
                let m1 = read_request(&mut reader).unwrap().expect("M1 arrives");
                respond(&mut stream, "470 Connection Authorization Required", m1.cseq(), &[]);
            }
            PairSetupScript::ErrorInM2(code) => {
                let m1 = read_request(&mut reader).unwrap().expect("M1 arrives");
                let body = Tlv8::new()
                    .push_u8(tlv_type::STATE, state::M2)
                    .push_u8(tlv_type::ERROR, code)
                    .encode();
                respond(&mut stream, "200 OK", m1.cseq(), &body);
            }
            PairSetupScript::DropAfterM2 => {
                let m1 = read_request(&mut reader).unwrap().expect("M1 arrives");
                respond(&mut stream, "200 OK", m1.cseq(), &SrpServer::new().m2());
                read_request(&mut reader).unwrap().expect("M3 arrives");
            }
        }
    });
    port
}

/// HKDF-SHA512 of `ikm` under `salt` and `info`, 32 bytes: HAP's derivation,
/// typed out apart from the sender's.
fn hap_hkdf(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha512>::new(Some(salt), ikm).expand(info, &mut out).unwrap();
    out
}

/// ChaCha20-Poly1305 under `key`, the nonce four zero bytes then `label`.
fn hap_cipher(key: &[u8; 32], label: &[u8; 8]) -> (ChaCha20Poly1305, Nonce) {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(label);
    (ChaCha20Poly1305::new(Key::from_slice(key)), Nonce::from(nonce))
}

fn hap_seal(key: &[u8; 32], label: &[u8; 8], plaintext: &[u8]) -> Vec<u8> {
    let (cipher, nonce) = hap_cipher(key, label);
    cipher.encrypt(&nonce, plaintext).unwrap()
}

fn hap_open(key: &[u8; 32], label: &[u8; 8], sealed: &[u8]) -> Vec<u8> {
    let (cipher, nonce) = hap_cipher(key, label);
    cipher.decrypt(&nonce, sealed).expect("the sender sealed it under the HAP key")
}

fn verifies(public: &[u8; 32], message: &[u8], signature: &[u8]) -> bool {
    let Ok(signature) = <[u8; 64]>::try_from(signature) else {
        return false;
    };
    VerifyingKey::from_bytes(public)
        .is_ok_and(|key| key.verify(message, &Signature::from_bytes(&signature)).is_ok())
}

/// An Error TLV reply at `state`.
pub(crate) fn error_reply(state: u8, code: u8) -> Vec<u8> {
    Tlv8::new().push_u8(tlv_type::STATE, state).push_u8(tlv_type::ERROR, code).encode()
}

/// The receiver's long-term pairing identity: its identifier and its Ed25519
/// key.
#[derive(Clone)]
pub(crate) struct ReceiverIdentity {
    pub(crate) id: Vec<u8>,
    seed: [u8; 32],
}

impl ReceiverIdentity {
    /// The identity a receiver has until it is reset.
    pub(crate) fn new() -> Self {
        ReceiverIdentity { id: b"58:55:CA:1A:2B:3C".to_vec(), seed: [0x5Eu8; 32] }
    }

    /// The identity the same receiver has after a reset: the same identifier,
    /// a new key.
    pub(crate) fn after_reset(&self) -> Self {
        ReceiverIdentity { id: self.id.clone(), seed: [0x6Fu8; 32] }
    }

    /// Another receiver's identity: another identifier, the same key.
    pub(crate) fn with_id(&self, id: &[u8]) -> Self {
        ReceiverIdentity { id: id.to_vec(), seed: self.seed }
    }

    pub(crate) fn public(&self) -> [u8; 32] {
        SigningKey::from_bytes(&self.seed).verifying_key().to_bytes()
    }

    fn sign(&self, message: &[u8]) -> Vec<u8> {
        SigningKey::from_bytes(&self.seed).sign(message).to_bytes().to_vec()
    }
}

/// A controller the receiver paired with: the identifier and Ed25519 public
/// key M5 carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairedController {
    pub(crate) id: Vec<u8>,
    pub(crate) public: [u8; 32],
}

/// The server half of a pair-setup with a code (M1 to M6).
pub(crate) struct CodeSetupServer {
    srp: SrpServer,
    receiver: ReceiverIdentity,
}

impl CodeSetupServer {
    pub(crate) fn new(code: &[u8], receiver: ReceiverIdentity) -> Self {
        CodeSetupServer { srp: SrpServer::with_password(code), receiver }
    }

    pub(crate) fn m2(&self) -> Vec<u8> {
        self.srp.m2()
    }

    /// M4: the server proof, or Authentication when the sender's code is
    /// another one, with `retry_delay` seconds to wait when it is set, as
    /// HomePod OS 27 answers.
    pub(crate) fn m4(&mut self, m3_body: &[u8], retry_delay: Option<u8>) -> Vec<u8> {
        self.srp.answer_m3(m3_body).unwrap_or_else(|| {
            let refusal = Tlv8::new()
                .push_u8(tlv_type::STATE, state::M4)
                .push_u8(tlv_type::ERROR, AUTHENTICATION);
            match retry_delay {
                Some(seconds) => refusal.push_u8(tlv_type::RETRY_DELAY, seconds),
                None => refusal,
            }
            .encode()
        })
    }

    /// Open the sender's M5, check its signature, and answer M6 with this
    /// receiver's identity; the controller it paired with, when the signature
    /// checks.
    pub(crate) fn m6(&self, m5_body: &[u8]) -> (Vec<u8>, Option<PairedController>) {
        let key = self.srp.shared_key().to_vec();
        let encrypt_key = hap_hkdf(b"Pair-Setup-Encrypt-Salt", &key, b"Pair-Setup-Encrypt-Info");
        let m5 = Tlv8::decode(m5_body).unwrap();
        assert_eq!(m5.get_u8(tlv_type::STATE), Some(state::M5));
        let inner = Tlv8::decode(&hap_open(
            &encrypt_key,
            b"PS-Msg05",
            m5.get(tlv_type::ENCRYPTED_DATA).unwrap(),
        ))
        .unwrap();
        let id = inner.get(tlv_type::IDENTIFIER).unwrap().to_vec();
        let public: [u8; 32] = inner.get(tlv_type::PUBLIC_KEY).unwrap().try_into().unwrap();
        let controller_x =
            hap_hkdf(b"Pair-Setup-Controller-Sign-Salt", &key, b"Pair-Setup-Controller-Sign-Info");
        let signed = [&controller_x[..], &id, &public].concat();
        if !verifies(&public, &signed, inner.get(tlv_type::SIGNATURE).unwrap()) {
            return (error_reply(state::M6, AUTHENTICATION), None);
        }

        let accessory_x =
            hap_hkdf(b"Pair-Setup-Accessory-Sign-Salt", &key, b"Pair-Setup-Accessory-Sign-Info");
        let receiver_public = self.receiver.public();
        let signature =
            self.receiver.sign(&[&accessory_x[..], &self.receiver.id, &receiver_public].concat());
        let reply_inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, self.receiver.id.clone())
            .push(tlv_type::PUBLIC_KEY, receiver_public.to_vec())
            .push(tlv_type::SIGNATURE, signature)
            .encode();
        let m6 = Tlv8::new()
            .push_u8(tlv_type::STATE, state::M6)
            .push(tlv_type::ENCRYPTED_DATA, hap_seal(&encrypt_key, b"PS-Msg06", &reply_inner))
            .encode();
        (m6, Some(PairedController { id, public }))
    }
}

/// The server half of a pair-verify (M1 to M4).
pub(crate) struct VerifyServer {
    receiver: ReceiverIdentity,
    secret: [u8; 32],
    exchange: Option<VerifyExchange>,
}

struct VerifyExchange {
    sender_public: [u8; 32],
    public: [u8; 32],
    shared: [u8; 32],
}

impl VerifyServer {
    pub(crate) fn new(receiver: ReceiverIdentity) -> Self {
        VerifyServer { receiver, secret: [0x2Du8; 32], exchange: None }
    }

    /// M2: this receiver's ephemeral, and its identity sealed under the
    /// X25519 secret.
    pub(crate) fn m2(&mut self, m1_body: &[u8]) -> Vec<u8> {
        let m1 = Tlv8::decode(m1_body).unwrap();
        assert_eq!(m1.get_u8(tlv_type::STATE), Some(state::M1));
        let sender_public: [u8; 32] = m1.get(tlv_type::PUBLIC_KEY).unwrap().try_into().unwrap();
        let public = x25519_dalek::x25519(self.secret, x25519_dalek::X25519_BASEPOINT_BYTES);
        let shared = x25519_dalek::x25519(self.secret, sender_public);
        let key = hap_hkdf(b"Pair-Verify-Encrypt-Salt", &shared, b"Pair-Verify-Encrypt-Info");
        let signature =
            self.receiver.sign(&[&public[..], &self.receiver.id, &sender_public[..]].concat());
        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, self.receiver.id.clone())
            .push(tlv_type::SIGNATURE, signature)
            .encode();
        self.exchange = Some(VerifyExchange { sender_public, public, shared });
        Tlv8::new()
            .push_u8(tlv_type::STATE, state::M2)
            .push(tlv_type::PUBLIC_KEY, public.to_vec())
            .push(tlv_type::ENCRYPTED_DATA, hap_seal(&key, b"PV-Msg02", &inner))
            .encode()
    }

    /// M4: accept the sender when it proves an identity in `known`, else
    /// Authentication. The shared secret, once accepted.
    pub(crate) fn m4(
        &self,
        m3_body: &[u8],
        known: &[PairedController],
    ) -> (Vec<u8>, Option<[u8; 32]>) {
        let exchange = self.exchange.as_ref().expect("M1 came before M3");
        let key =
            hap_hkdf(b"Pair-Verify-Encrypt-Salt", &exchange.shared, b"Pair-Verify-Encrypt-Info");
        let m3 = Tlv8::decode(m3_body).unwrap();
        assert_eq!(m3.get_u8(tlv_type::STATE), Some(state::M3));
        let inner =
            Tlv8::decode(&hap_open(&key, b"PV-Msg03", m3.get(tlv_type::ENCRYPTED_DATA).unwrap()))
                .unwrap();
        let id = inner.get(tlv_type::IDENTIFIER).unwrap();
        let signature = inner.get(tlv_type::SIGNATURE).unwrap();
        let signed = [&exchange.sender_public[..], id, &exchange.public[..]].concat();
        let accepted = known.iter().any(|controller| {
            controller.id == id && verifies(&controller.public, &signed, signature)
        });
        if accepted {
            (Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode(), Some(exchange.shared))
        } else {
            (error_reply(state::M4, AUTHENTICATION), None)
        }
    }
}

/// What a scripted receiver that pairs by code refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeRefusal {
    /// Answer a code pair-setup's M1 with this HomeKit error.
    SetupM2(u8),
    /// Answer pair-verify's M1 with HTTP 470.
    VerifyM1With470,
    /// Answer pair-verify's M1 with this HomeKit error.
    VerifyM2(u8),
}

/// The code a scripted receiver takes.
#[derive(Debug, Clone)]
pub(crate) enum ReceiverCode {
    /// A PIN it shows on each `/pair-pin-start`: the first entry on the first,
    /// the next on the next, as a receiver shows a new PIN after a wrong one.
    Pins(Vec<String>),
    /// A password set on it; it takes no `/pair-pin-start`.
    Password(String),
}

/// A receiver that pairs by code and pair-verifies the controllers it paired
/// with, over the requests one connection carries. Every request must carry
/// `X-Apple-HKP: 3` and name the sender in `X-Apple-Client-Name`, without
/// which tvOS 27 shows no PIN.
pub(crate) struct CodeReceiver {
    pub(crate) identity: ReceiverIdentity,
    code: ReceiverCode,
    /// How many PINs it has shown.
    pins_shown: usize,
    /// The sender's name it showed beside the last PIN.
    pub(crate) pin_shown_beside: Option<String>,
    pub(crate) refusal: Option<CodeRefusal>,
    /// The seconds a wrong code's Authentication asks the sender to wait.
    pub(crate) retry_delay: Option<u8>,
    /// The controllers it paired with, which pair-verify accepts.
    pub(crate) paired: Vec<PairedController>,
    /// The URI of each request, in order.
    pub(crate) requests: Vec<String>,
    setup: Option<CodeSetupServer>,
    verify: Option<VerifyServer>,
}

/// How serving one connection ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Served {
    /// The sender closed the connection.
    Closed,
    /// A pair-verify finished with this shared secret; what follows on the
    /// connection is encrypted.
    Verified([u8; 32]),
}

impl CodeReceiver {
    pub(crate) fn new(code: ReceiverCode) -> Self {
        CodeReceiver {
            identity: ReceiverIdentity::new(),
            code,
            pins_shown: 0,
            pin_shown_beside: None,
            refusal: None,
            retry_delay: None,
            paired: Vec::new(),
            requests: Vec::new(),
            setup: None,
            verify: None,
        }
    }

    /// Serve pairing requests on one connection until the sender closes it
    /// or a pair-verify finishes.
    pub(crate) fn serve(
        &mut self,
        reader: &mut impl std::io::BufRead,
        writer: &mut TcpStream,
    ) -> Served {
        while let Some(request) = read_request(reader).unwrap() {
            self.requests.push(request.uri.clone());
            assert_eq!(request.header("X-Apple-HKP"), Some("3"), "{} carries HKP 3", request.uri);
            assert!(
                request.header("X-Apple-Client-Name").is_some_and(|name| !name.is_empty()),
                "{} names the sender",
                request.uri
            );
            let (status, body, verified) = self.answer(&request);
            respond(writer, status, request.cseq(), &body);
            if let Some(shared) = verified {
                return Served::Verified(shared);
            }
        }
        Served::Closed
    }

    fn answer(&mut self, request: &ParsedRequest) -> (&'static str, Vec<u8>, Option<[u8; 32]>) {
        const OK: &str = "200 OK";
        let step = || Tlv8::decode(&request.body).unwrap().get_u8(tlv_type::STATE);
        match request.uri.as_str() {
            "/pair-pin-start" => {
                assert!(
                    matches!(self.code, ReceiverCode::Pins(_)),
                    "a password takes no PIN start"
                );
                self.pins_shown += 1;
                self.pin_shown_beside = request.header("X-Apple-Client-Name").map(String::from);
                (OK, Vec::new(), None)
            }
            "/pair-setup" => match step() {
                Some(state::M1) => {
                    let m1 = Tlv8::decode(&request.body).unwrap();
                    assert_eq!(m1.get(tlv_type::FLAGS), None, "a code pair-setup has no flags");
                    if let Some(CodeRefusal::SetupM2(code)) = self.refusal {
                        return (OK, error_reply(state::M2, code), None);
                    }
                    let setup = CodeSetupServer::new(self.current_code(), self.identity.clone());
                    let m2 = setup.m2();
                    self.setup = Some(setup);
                    (OK, m2, None)
                }
                Some(state::M3) => {
                    (OK, self.setup.as_mut().unwrap().m4(&request.body, self.retry_delay), None)
                }
                Some(state::M5) => {
                    let (m6, paired) = self.setup.take().unwrap().m6(&request.body);
                    if let Some(paired) = paired {
                        self.paired.retain(|known| known.id != paired.id);
                        self.paired.push(paired);
                    }
                    (OK, m6, None)
                }
                other => panic!("unexpected pair-setup state {other:?}"),
            },
            "/pair-verify" => match step() {
                Some(state::M1) => match self.refusal {
                    Some(CodeRefusal::VerifyM1With470) => {
                        ("470 Connection Authorization Required", Vec::new(), None)
                    }
                    Some(CodeRefusal::VerifyM2(code)) => (OK, error_reply(state::M2, code), None),
                    _ => {
                        let mut verify = VerifyServer::new(self.identity.clone());
                        let m2 = verify.m2(&request.body);
                        self.verify = Some(verify);
                        (OK, m2, None)
                    }
                },
                Some(state::M3) => {
                    let (m4, shared) = self.verify.take().unwrap().m4(&request.body, &self.paired);
                    (OK, m4, shared)
                }
                other => panic!("unexpected pair-verify state {other:?}"),
            },
            other => panic!("unexpected request {other}"),
        }
    }

    /// The PIN it shows now, or its password.
    fn current_code(&self) -> &[u8] {
        match &self.code {
            ReceiverCode::Pins(pins) => {
                let shown = self.pins_shown.checked_sub(1).expect("a PIN start came first");
                pins[shown].as_bytes()
            }
            ReceiverCode::Password(password) => password.as_bytes(),
        }
    }
}
