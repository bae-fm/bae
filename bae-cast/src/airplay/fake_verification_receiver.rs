//! The AirPlay 1 receiver the device verification tests talk to: it shows a
//! PIN, pairs over SRP written apart from [`super::srp::SrpClient`], opens the
//! key bae seals, verifies bae at each connection, and answers the RAOP
//! session's requests.
//!
//! Its AES-GCM, AES-CTR, X25519 and Ed25519 calls and their labels are typed
//! out apart from [`super::device_verification`], so a sender that agrees
//! with it agrees with pyatv's and OwnTone's reading of the protocol, not only
//! with its own helpers.

use std::io::BufRead;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha512};

use super::bplist::{self, Plist};
use super::fake_raop_receiver::{DigestGate, CHALLENGE};
use super::fake_receiver::{respond_with_headers, SrpServer};
use super::rtsp::{read_request, Method, ParsedRequest};

/// A RAOP receiver that verifies the device: the PINs it shows, the devices
/// it paired with, and what it was asked.
pub(crate) struct VerificationReceiver {
    /// The PINs it shows, one on each `/pair-pin-start`, as a receiver shows a
    /// new PIN after a wrong one.
    pins: Vec<String>,
    /// How many it has shown.
    shown: usize,
    /// The sender's name it showed beside the last PIN.
    pub(crate) shown_beside: Option<String>,
    /// The Ed25519 public keys of the devices it paired with, which
    /// pair-verify accepts.
    pub(crate) paired: Vec<[u8; 32]>,
    /// Whether it answers OPTIONS on a connection that didn't verify with
    /// 403, as a receiver that asks for device verification without
    /// advertising it does.
    pub(crate) forbids_unverified: bool,
    /// The status line it answers pair-verify's first step with, in place of
    /// its keys.
    pub(crate) refuse_verify_keys: Option<&'static str>,
    /// Each request it took, as its method and URI, in order.
    pub(crate) requests: Vec<String>,
    /// The RTP ports its SETUP names.
    pub(crate) ports: (u16, u16),
    /// The password it asks for as HTTP Digest, on the requests it says.
    pub(crate) password: Option<DigestGate>,
    setup: Option<SrpServer>,
}

/// One connection's pair-verify, between its two steps.
struct Verifying {
    sender_public: [u8; 32],
    sender_key: [u8; 32],
    public: [u8; 32],
    shared: [u8; 32],
}

/// The receiver's own long-term Ed25519 key, which signs its half of
/// pair-verify.
const RECEIVER_SEED: [u8; 32] = [0x4Bu8; 32];
/// The receiver's X25519 ephemeral secret.
const RECEIVER_EPHEMERAL: [u8; 32] = [0x3Cu8; 32];

impl VerificationReceiver {
    /// A receiver that shows `pins`, paired with nobody.
    pub(crate) fn new(pins: &[&str]) -> Self {
        VerificationReceiver {
            pins: pins.iter().map(ToString::to_string).collect(),
            shown: 0,
            shown_beside: None,
            paired: Vec::new(),
            forbids_unverified: false,
            refuse_verify_keys: None,
            requests: Vec::new(),
            ports: (1, 1),
            password: None,
            setup: None,
        }
    }

    /// Forget every device it paired with, as a receiver that was reset does.
    pub(crate) fn forget_paired(&mut self) {
        self.paired.clear();
    }

    /// How many requests went to `uri`.
    pub(crate) fn count(&self, uri: &str) -> usize {
        self.requests.iter().filter(|request| request.ends_with(&format!(" {uri}"))).count()
    }

    /// Serve the requests of one connection until the sender closes it.
    pub(crate) fn serve(&mut self, reader: &mut impl BufRead, writer: &mut TcpStream) {
        let mut verifying = None;
        let mut verified = false;
        while let Some(request) = read_request(reader).unwrap() {
            self.requests.push(format!("{} {}", request.method.as_str(), request.uri));
            let cseq = request.cseq().to_string();
            if let Some(gate) = &mut self.password {
                if !gate.lets_through(&request) {
                    respond_with_headers(writer, "401 Unauthorized", &cseq, CHALLENGE, &[]);
                    continue;
                }
            }
            let (status, extra, body) = match (&request.method, request.uri.as_str()) {
                (Method::Post, "/pair-pin-start") => {
                    self.shown += 1;
                    self.shown_beside = request.header("X-Apple-Client-Name").map(String::from);
                    ("200 OK", String::new(), Vec::new())
                }
                (Method::Post, "/pair-setup-pin") => {
                    assert_eq!(
                        request.header("Content-Type"),
                        Some("application/x-apple-binary-plist")
                    );
                    let (status, body) = self.answer_setup(&request);
                    (status, String::new(), body)
                }
                (Method::Post, "/pair-verify") => {
                    assert_eq!(request.header("Content-Type"), Some("application/octet-stream"));
                    let (status, body) =
                        self.answer_verify(&request.body, &mut verifying, &mut verified);
                    (status, String::new(), body)
                }
                (Method::Options, _) if self.forbids_unverified && !verified => {
                    ("403 Forbidden", String::new(), Vec::new())
                }
                (Method::Setup, _) => {
                    let (audio, control) = self.ports;
                    let transport = format!(
                        "Transport: RTP/AVP/UDP;unicast;server_port={audio};\
                         control_port={control};timing_port=1\r\n"
                    );
                    ("200 OK", transport, Vec::new())
                }
                (
                    Method::Options
                    | Method::Announce
                    | Method::Record
                    | Method::SetParameter
                    | Method::Flush
                    | Method::Teardown,
                    _,
                ) => ("200 OK", String::new(), Vec::new()),
                (method, uri) => panic!("unexpected request {} {uri}", method.as_str()),
            };
            respond_with_headers(writer, status, &cseq, &extra, &body);
        }
    }

    /// Answer one `/pair-setup-pin` step: the username starts SRP over the
    /// PIN shown now, the proof is checked against it, and the sealed key is
    /// opened and kept.
    fn answer_setup(&mut self, request: &ParsedRequest) -> (&'static str, Vec<u8>) {
        let body = bplist::decode(&request.body).expect("the sender sends a binary plist");
        let data = |key| body.get(key).and_then(Plist::as_data);
        if let Some(Plist::String(user)) = body.get("user") {
            assert_eq!(body.get("method"), Some(&Plist::String("pin".to_string())));
            let last = self.shown.checked_sub(1).expect("a PIN start came first");
            let server = SrpServer::airplay1(user.as_bytes(), self.pins[last].as_bytes());
            let (salt, b_pub) = server.salt_and_public();
            self.setup = Some(server);
            return ("200 OK", plist(&[("pk", b_pub), ("salt", salt)]));
        }
        if let (Some(a_pub), Some(proof)) = (data("pk"), data("proof")) {
            let server = self.setup.as_mut().expect("the username came first");
            return match server.prove(a_pub, proof) {
                Some(m2) => ("200 OK", plist(&[("proof", m2)])),
                // pyatv issue 1548: an Apple TV answers a wrong PIN with 500.
                None => ("500 Internal Server Error", Vec::new()),
            };
        }
        let (Some(sealed), Some(tag)) = (data("epk"), data("authTag")) else {
            panic!("unexpected pair-setup-pin body {body:?}");
        };
        let server = self.setup.take().expect("the proof came first");
        let key = server.shared_key();
        let aes_key = &Sha512::digest([b"Pair-Setup-AES-Key".as_slice(), key].concat())[..16];
        let mut iv = Sha512::digest([b"Pair-Setup-AES-IV".as_slice(), key].concat())[..16].to_vec();
        iv[15] = iv[15].wrapping_add(1);
        let public = aes_gcm::AesGcm::<aes::Aes128, U16>::new_from_slice(aes_key)
            .unwrap()
            .decrypt(
                aes_gcm::Nonce::<U16>::from_slice(&iv),
                Payload { msg: &[sealed, tag].concat(), aad: &[] },
            )
            .expect("the sender sealed its key under the SRP key");
        let public: [u8; 32] = public.try_into().expect("an Ed25519 public key");
        self.paired.retain(|known| *known != public);
        self.paired.push(public);
        ("200 OK", Vec::new())
    }

    /// Answer one `/pair-verify` step: the sender's keys get this receiver's
    /// ephemeral and its encrypted signature; the sender's encrypted
    /// signature is accepted when it checks with a key this receiver paired
    /// with.
    fn answer_verify(
        &mut self,
        body: &[u8],
        verifying: &mut Option<Verifying>,
        verified: &mut bool,
    ) -> (&'static str, Vec<u8>) {
        let (marker, rest) = body.split_at(4);
        if marker == [1, 0, 0, 0] {
            if let Some(status) = self.refuse_verify_keys {
                return (status, Vec::new());
            }
            let sender_public: [u8; 32] = rest[..32].try_into().unwrap();
            let sender_key: [u8; 32] = rest[32..64].try_into().unwrap();
            let public =
                x25519_dalek::x25519(RECEIVER_EPHEMERAL, x25519_dalek::X25519_BASEPOINT_BYTES);
            let shared = x25519_dalek::x25519(RECEIVER_EPHEMERAL, sender_public);
            let mut signature = SigningKey::from_bytes(&RECEIVER_SEED)
                .sign(&[public, sender_public].concat())
                .to_bytes();
            ctr_cipher(&shared).apply_keystream(&mut signature);
            *verifying = Some(Verifying { sender_public, sender_key, public, shared });
            return ("200 OK", [&public[..], &signature].concat());
        }
        assert_eq!(marker, [0, 0, 0, 0], "pair-verify's second step");
        let exchange = verifying.take().expect("the keys came first");
        let mut signature: [u8; 64] = rest.try_into().expect("a 64-byte signature");
        let mut cipher = ctr_cipher(&exchange.shared);
        cipher.seek(64u64);
        cipher.apply_keystream(&mut signature);
        let signed = [exchange.sender_public, exchange.public].concat();
        let checks = VerifyingKey::from_bytes(&exchange.sender_key)
            .is_ok_and(|key| key.verify(&signed, &Signature::from_bytes(&signature)).is_ok());
        if checks && self.paired.contains(&exchange.sender_key) {
            *verified = true;
            ("200 OK", Vec::new())
        } else {
            ("403 Forbidden", Vec::new())
        }
    }
}

/// AES-128-CTR under the pair-verify key and IV of `shared`.
fn ctr_cipher(shared: &[u8; 32]) -> ctr::Ctr128BE<aes::Aes128> {
    let key = &Sha512::digest([b"Pair-Verify-AES-Key".as_slice(), shared].concat())[..16];
    let iv = &Sha512::digest([b"Pair-Verify-AES-IV".as_slice(), shared].concat())[..16];
    ctr::Ctr128BE::<aes::Aes128>::new_from_slices(key, iv).unwrap()
}

/// A binary plist dict of data values.
fn plist(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    bplist::encode(&Plist::Dict(
        entries.iter().map(|(key, value)| (key.to_string(), Plist::Data(value.clone()))).collect(),
    ))
}

/// Listen on a loopback port and serve every connection a sender opens with
/// `receiver`, one after another on one thread, for as long as the test
/// runs. Returns the port and the receiver, which the test reads.
pub(crate) fn listen_for_verification(
    receiver: VerificationReceiver,
) -> (u16, Arc<Mutex<VerificationReceiver>>) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind a loopback port");
    let port = listener.local_addr().expect("read the bound port").port();
    let receiver = Arc::new(Mutex::new(receiver));
    let serving = receiver.clone();
    bae_util::test_spawn::thread(move || {
        for stream in listener.incoming() {
            let mut stream = stream.expect("the sender connects");
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            serving.lock().unwrap().serve(&mut reader, &mut stream);
        }
    });
    (port, receiver)
}
