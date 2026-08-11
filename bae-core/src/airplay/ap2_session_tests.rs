use super::*;
use crate::airplay::ap2_channel::control_keys;
use std::io::{BufRead, Write};
use std::net::TcpListener;

#[test]
fn setup_stream_body_carries_the_audio_key() {
    let shk = [0x7Bu8; 32];
    let body = setup_stream_body(&shk, 6001, 6002);
    let decoded = bplist::decode(&bplist::encode(&body)).unwrap();
    let stream = match decoded.get("streams").unwrap() {
        Plist::Array(items) => &items[0],
        _ => panic!("streams is an array"),
    };
    assert_eq!(stream.get("shk").unwrap(), &Plist::Data(shk.to_vec()));
    assert_eq!(stream.get("spf").unwrap().as_integer(), Some(352));
    assert_eq!(stream.get("controlPort").unwrap().as_integer(), Some(6001));
}

#[test]
fn set_rate_anchor_pauses_and_resumes() {
    let pause = set_rate_anchor_body(0, 0, None);
    assert_eq!(pause.get("rate").unwrap().as_integer(), Some(0));
    assert!(pause.get("networkTimeSecs").is_none());

    let resume = set_rate_anchor_body(1, 44_100, Some((123, 456, 0x99)));
    assert_eq!(resume.get("rate").unwrap().as_integer(), Some(1));
    assert_eq!(resume.get("rtpTime").unwrap().as_integer(), Some(44_100));
    assert_eq!(
        resume.get("networkTimeSecs").unwrap().as_integer(),
        Some(123)
    );
    assert_eq!(
        resume.get("networkTimeTimelineID").unwrap().as_integer(),
        Some(0x99)
    );
}

#[test]
fn set_peers_body_is_an_ip_array() {
    let peers = vec!["10.0.0.2".parse().unwrap(), "10.0.0.9".parse().unwrap()];
    let body = set_peers_body(&peers);
    match body {
        Plist::Array(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].as_string(), Some("10.0.0.2"));
        }
        _ => panic!("SETPEERS is an array"),
    }
}

#[test]
fn parse_stream_ports_reads_the_response() {
    let response = Plist::Dict(vec![(
        "streams".to_string(),
        Plist::Array(vec![Plist::Dict(vec![
            ("dataPort".to_string(), Plist::Integer(7000)),
            ("controlPort".to_string(), Plist::Integer(7001)),
        ])]),
    )]);
    let ports = parse_stream_ports(&response).unwrap();
    assert_eq!(ports.data_port, 7000);
    assert_eq!(ports.control_port, 7001);
}

/// The whole control sequence runs over a real encrypted socket against a
/// scripted fake receiver that decrypts each request with the pair-verify
/// key, asserts the order and the decoded plist bodies, and replies encrypted.
#[test]
fn setup_sequence_runs_over_the_encrypted_channel() {
    let shared = [0x33u8; 32];
    let shk = Ap2Control::audio_key(&shared);
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        // The receiver's write key is the sender's read key and vice versa.
        let (write, read) = control_keys(&shared);
        let mut channel = HapChannel::from_keys(&read, &write);

        let mut methods = Vec::new();
        let mut saw_shk = false;
        let mut saw_peers = false;
        let mut saw_anchor_rate = None;

        for step in 0..5 {
            let (method, cseq, body) = read_encrypted_request(&mut reader, &mut channel);
            match method.as_str() {
                "SETUP" if body.get("streams").is_some() => {
                    let stream = match body.get("streams").unwrap() {
                        Plist::Array(items) => items[0].clone(),
                        _ => unreachable!(),
                    };
                    saw_shk = stream.get("shk") == Some(&Plist::Data(shk.to_vec()));
                }
                "SETPEERS" => saw_peers = matches!(body, Plist::Array(_)),
                "SETRATEANCHORTIME" => {
                    saw_anchor_rate = body.get("rate").and_then(Plist::as_integer);
                }
                _ => {}
            }
            methods.push(method.clone());

            // Reply. SETUP(session) returns the event port; SETUP(stream)
            // returns the receiver's audio/control ports; the rest are empty.
            let body_bytes = match (method.as_str(), step) {
                ("SETUP", 0) => bplist::encode(&Plist::Dict(vec![(
                    "eventPort".to_string(),
                    Plist::Integer(7010),
                )])),
                ("SETUP", 1) => bplist::encode(&Plist::Dict(vec![(
                    "streams".to_string(),
                    Plist::Array(vec![Plist::Dict(vec![
                        ("dataPort".to_string(), Plist::Integer(7000)),
                        ("controlPort".to_string(), Plist::Integer(7001)),
                    ])]),
                )])),
                _ => Vec::new(),
            };
            let response = build_response(&cseq, &body_bytes);
            writer.write_all(&channel.seal(&response)).unwrap();
            writer.flush().unwrap();
        }
        (methods, saw_shk, saw_peers, saw_anchor_rate)
    });

    let stream = TcpStream::connect(addr).unwrap();
    let local_ip = stream.local_addr().unwrap().ip();
    let mut control = Ap2Control::new(stream, &shared, "rtsp://x/1", "bae/test").unwrap();
    let params = Ap2SetupParams {
        timing_protocol: "NTP".to_string(),
        group_uuid: "group".to_string(),
        session_uuid: "session".to_string(),
        mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
        local_ip,
        local_control_port: 6001,
        local_timing_port: 6002,
        peers: vec![local_ip, addr.ip()],
        ptp_anchor: None,
    };
    let ports = control.run_setup_sequence(&shk, &params).unwrap();
    assert_eq!(ports.data_port, 7000);
    assert_eq!(ports.control_port, 7001);

    let (methods, saw_shk, saw_peers, saw_anchor_rate) = server.join().unwrap();
    assert_eq!(
        methods,
        vec!["SETUP", "SETUP", "SETPEERS", "SETRATEANCHORTIME", "RECORD"]
    );
    assert!(
        saw_shk,
        "the SETUP stream body carried the derived audio key"
    );
    assert!(saw_peers, "SETPEERS carried an IP array");
    assert_eq!(saw_anchor_rate, Some(1), "SETRATEANCHORTIME rate was play");
}

/// Read one encrypted RTSP request from the fake receiver's socket: decrypt
/// blocks until a full request is parsed, returning method, CSeq, and the
/// decoded plist body (empty dict when there's no body).
fn read_encrypted_request(
    reader: &mut impl Read,
    channel: &mut HapChannel,
) -> (String, String, Plist) {
    let mut plaintext = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(parsed) = try_parse_request(&plaintext) {
            return parsed;
        }
        let read = reader.read(&mut chunk).unwrap();
        assert!(read > 0, "receiver socket closed early");
        channel.feed(&chunk[..read]);
        while let Some(block) = channel.next_block().unwrap() {
            plaintext.extend_from_slice(&block);
        }
    }
}

/// Parse an RTSP request from decrypted plaintext, or `None` if incomplete.
fn try_parse_request(bytes: &[u8]) -> Option<(String, String, Plist)> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut line = String::new();
    if cursor.read_line(&mut line).ok()? == 0 || !line.ends_with('\n') {
        return None;
    }
    let method = line.split(' ').next()?.to_string();
    let mut cseq = String::new();
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if cursor.read_line(&mut header).ok()? == 0 || !header.ends_with('\n') {
            return None;
        }
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "cseq" => cseq = v.trim().to_string(),
                "content-length" => {
                    content_length = v.trim().parse().expect("valid content-length")
                }
                _ => {}
            }
        }
    }
    let consumed = cursor.position() as usize;
    if bytes.len() < consumed + content_length {
        return None;
    }
    let body_bytes = &bytes[consumed..consumed + content_length];
    let body = if content_length == 0 {
        Plist::Dict(Vec::new())
    } else {
        bplist::decode(body_bytes).unwrap()
    };
    Some((method, cseq, body))
}

fn build_response(cseq: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

// -- Full AirPlay 2 session against a scripted fake receiver --

use crate::airplay::secure_rng::SecureRng;
use crate::airplay::srp::SrpGroup;
use crate::airplay::stream::PcmSource;
use crate::airplay::tlv8::{state, tlv_type, Tlv8};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use std::net::UdpSocket;
use std::time::Duration;

/// A source that yields `packets` packets of full-scale stereo frames (each
/// sample `0x0102`), then nothing.
struct FixedSource {
    packets_left: usize,
}
impl PcmSource for FixedSource {
    fn next_frames(&mut self, out: &mut [i16]) -> usize {
        if self.packets_left == 0 {
            return 0;
        }
        self.packets_left -= 1;
        out.fill(0x0102);
        out.len() / 2
    }
}

fn pv_nonce(label: &[u8; 8]) -> Nonce {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(label);
    Nonce::from(b)
}

/// Read one plaintext RTSP request (method, path, CSeq, body) from the pairing
/// phase.
fn read_plaintext_request(reader: &mut impl BufRead) -> (String, String, String, Vec<u8>) {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut cseq = String::new();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).unwrap();
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "cseq" => cseq = v.trim().to_string(),
                "content-length" => len = v.trim().parse().expect("valid content-length"),
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut body).unwrap();
    (method, path, cseq, body)
}

/// What the fake receiver captured over one full session.
struct Captured {
    methods: Vec<String>,
    saw_shk: bool,
    timing_protocol: String,
    anchor_has_network_time: bool,
    pcm: Vec<u8>,
}

/// Run a whole AirPlay 2 session against the scripted fake receiver with the
/// sender using `timing`, returning what the receiver saw.
fn run_full_ap2_session(timing: super::super::airplay2::TimingProtocol) -> Captured {
    let tcp = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = tcp.local_addr().unwrap();
    // Bind the audio (data) socket up front so its real port rides in SETUP.
    let data_udp = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    data_udp
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let data_port = data_udp.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let (stream, _) = tcp.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        // -- Pairing (plaintext). The SRP secret is discarded by the sender, so
        // a valid B and a bare M4 suffice; pair-verify's X25519 secret is real.
        let group = SrpGroup::rfc5054_3072();
        let b_pub = group
            .g
            .modpow(&num_bigint::BigUint::from_bytes_be(&[0x7Cu8; 32]), &group.n);
        let respond = |writer: &mut TcpStream, cseq: &str, body: Vec<u8>| {
            let mut out = format!(
                "RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .into_bytes();
            out.extend_from_slice(&body);
            writer.write_all(&out).unwrap();
            writer.flush().unwrap();
        };

        // pair-setup M1 -> M2 (salt + B)
        let (_m, _p, cseq, _b) = read_plaintext_request(&mut reader);
        respond(
            &mut writer,
            &cseq,
            Tlv8::new()
                .push_u8(tlv_type::STATE, state::M2)
                .push(tlv_type::SALT, vec![0xA5u8; 16])
                .push(tlv_type::PUBLIC_KEY, b_pub.to_bytes_be())
                .encode(),
        );
        // pair-setup M3 -> M4 (bare)
        let (_m, _p, cseq, _b) = read_plaintext_request(&mut reader);
        respond(
            &mut writer,
            &cseq,
            Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode(),
        );

        // pair-verify M1 -> M2 (device eph + signed encrypted identity)
        use ed25519_dalek::{Signer, SigningKey};
        use x25519_dalek::{PublicKey, StaticSecret};
        let eph = StaticSecret::random_from_rng(SecureRng);
        let eph_pub = PublicKey::from(&eph);
        let signing = SigningKey::generate(&mut SecureRng);
        let (_m, _p, cseq, m1) = read_plaintext_request(&mut reader);
        let m1_tlv = Tlv8::decode(&m1).unwrap();
        let sender_pub: [u8; 32] = m1_tlv
            .get(tlv_type::PUBLIC_KEY)
            .unwrap()
            .try_into()
            .unwrap();
        let shared = eph.diffie_hellman(&PublicKey::from(sender_pub)).to_bytes();
        let session_key = crate::airplay::ap2_channel::hkdf32(
            b"Pair-Verify-Encrypt-Salt",
            &shared,
            b"Pair-Verify-Encrypt-Info",
        );
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&session_key));
        let mut signed = Vec::new();
        signed.extend_from_slice(eph_pub.as_bytes());
        signed.extend_from_slice(&sender_pub);
        let sig = signing.sign(&signed);
        let inner = Tlv8::new()
            .push(tlv_type::IDENTIFIER, b"fake".to_vec())
            .push(tlv_type::SIGNATURE, sig.to_bytes().to_vec())
            .encode();
        let sealed = cipher
            .encrypt(
                &pv_nonce(b"PV-Msg02"),
                Payload {
                    msg: &inner,
                    aad: &[],
                },
            )
            .unwrap();
        respond(
            &mut writer,
            &cseq,
            Tlv8::new()
                .push_u8(tlv_type::STATE, state::M2)
                .push(tlv_type::PUBLIC_KEY, eph_pub.as_bytes().to_vec())
                .push(tlv_type::ENCRYPTED_DATA, sealed)
                .encode(),
        );
        // pair-verify M3 -> M4 (bare)
        let (_m, _p, cseq, _m3) = read_plaintext_request(&mut reader);
        respond(
            &mut writer,
            &cseq,
            Tlv8::new().push_u8(tlv_type::STATE, state::M4).encode(),
        );

        // -- Encrypted control channel (keys derived from the verify secret).
        let (write, read) = control_keys(&shared);
        let mut channel = HapChannel::from_keys(&read, &write);
        let shk = audio_key(&shared);

        let mut methods = Vec::new();
        let mut saw_shk = false;
        let mut timing_protocol = String::new();
        let mut anchor_has_network_time = false;
        for step in 0..5 {
            let (method, cseq, body) = read_encrypted_request(&mut reader, &mut channel);
            if method == "SETUP" && body.get("streams").is_some() {
                if let Plist::Array(items) = body.get("streams").unwrap() {
                    saw_shk = items[0].get("shk") == Some(&Plist::Data(shk.to_vec()));
                }
            }
            if method == "SETUP" && body.get("timingProtocol").is_some() {
                timing_protocol = body
                    .get("timingProtocol")
                    .and_then(Plist::as_string)
                    .unwrap_or("")
                    .to_string();
            }
            if method == "SETRATEANCHORTIME" {
                anchor_has_network_time = body.get("networkTimeSecs").is_some();
            }
            methods.push(method.clone());
            let reply = match (method.as_str(), step) {
                ("SETUP", 0) => bplist::encode(&Plist::Dict(vec![(
                    "eventPort".to_string(),
                    Plist::Integer(1),
                )])),
                ("SETUP", 1) => bplist::encode(&Plist::Dict(vec![(
                    "streams".to_string(),
                    Plist::Array(vec![Plist::Dict(vec![
                        ("dataPort".to_string(), Plist::Integer(u64::from(data_port))),
                        ("controlPort".to_string(), Plist::Integer(1)),
                    ])]),
                )])),
                _ => Vec::new(),
            };
            writer
                .write_all(&channel.seal(&build_response(&cseq, &reply)))
                .unwrap();
            writer.flush().unwrap();
        }

        // -- Receive one audio datagram and decrypt it with the shk-derived key.
        let mut buf = [0u8; 4096];
        let (len, _from) = data_udp.recv_from(&mut buf).unwrap();
        let datagram = &buf[..len];
        let header = &datagram[..12];
        let nonce_tail = &datagram[len - 8..];
        let ciphertext = &datagram[12..len - 8];
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(nonce_tail);
        let audio_cipher = ChaCha20Poly1305::new(Key::from_slice(&shk));
        let pcm = audio_cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: ciphertext,
                    aad: &header[4..12],
                },
            )
            .expect("audio datagram decrypts with the shk-derived key");

        Captured {
            methods,
            saw_shk,
            timing_protocol,
            anchor_has_network_time,
            pcm,
        }
    });

    // The sender: pair, set up, and stream five packets of known PCM.
    let session = Ap2Session::start(
        addr.ip(),
        addr.port(),
        Box::new(FixedSource { packets_left: 5 }),
        Some(88_200),
        timing,
        Arc::new(crate::airplay::stream::SystemClock::new()),
    )
    .expect("AirPlay 2 session starts");

    let captured = server.join().unwrap();
    drop(session);
    captured
}

/// A full NTP session: the control order, the shk, and the decrypted PCM, with
/// NTP timing (no network-time anchor).
#[test]
fn full_ap2_session_pairs_sets_up_and_streams_encrypted_audio() {
    let c = run_full_ap2_session(super::super::airplay2::TimingProtocol::Ntp);
    assert_eq!(
        c.methods,
        vec!["SETUP", "SETUP", "SETPEERS", "SETRATEANCHORTIME", "RECORD"],
        "the AirPlay 2 control sequence runs in order over the encrypted channel"
    );
    assert!(
        c.saw_shk,
        "SETUP carried the shk audio key derived from pair-verify"
    );
    assert_eq!(c.timing_protocol, "NTP");
    assert!(
        !c.anchor_has_network_time,
        "NTP carries no network-time anchor"
    );
    // One packet = 352 stereo frames of 0x0102, little-endian.
    assert_eq!(c.pcm.len(), 352 * 2 * 2);
    assert_eq!(&c.pcm[..4], &[0x02, 0x01, 0x02, 0x01]);
}

/// A PTP-required receiver: SETUP advertises PTP and SETRATEANCHORTIME carries
/// the network-time anchor — the decision `TimingProtocol::from_features` makes.
#[test]
fn ptp_receiver_gets_ptp_timing_and_the_network_anchor() {
    let c = run_full_ap2_session(super::super::airplay2::TimingProtocol::Ptp);
    assert_eq!(c.timing_protocol, "PTP");
    assert!(
        c.anchor_has_network_time,
        "PTP carries the network-time anchor in SETRATEANCHORTIME"
    );
}
