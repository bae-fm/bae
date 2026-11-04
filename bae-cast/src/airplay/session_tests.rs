use super::*;
use crate::airplay::crypto::apple_public_key;
use bae_playback::airplay_output::AirPlayStreamControl;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

#[test]
fn volume_maps_to_raop_db_range() {
    let level = |level: f32| bae_playback::Volume::new(level).unwrap();
    assert_eq!(volume_to_raop_db(level(1.0)), 0.0);
    assert_eq!(volume_to_raop_db(level(0.5)), -15.0);
    assert_eq!(volume_to_raop_db(level(0.0)), -144.0);
}

#[test]
fn sdp_is_l16_and_omits_keys_when_unencrypted() {
    let sdp = build_announce_sdp(
        42,
        "10.0.0.2".parse().unwrap(),
        "10.0.0.9".parse().unwrap(),
        &RaopCipher::none(),
    );
    assert!(sdp.contains("a=rtpmap:96 L16/44100/2\r\n"));
    assert!(sdp.contains("a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n"));
    assert!(sdp.contains("o=iTunes 42 0 IN IP4 10.0.0.2\r\n"));
    assert!(sdp.contains("c=IN IP4 10.0.0.9\r\n"));
    assert!(!sdp.contains("rsaaeskey"));
    assert!(!sdp.contains("aesiv"));
}

#[test]
fn sdp_carries_keys_when_encrypted() {
    let cipher = RaopCipher::from_key_iv(&apple_public_key(), [0x11; 16], [0x22; 16]).unwrap();
    let sdp =
        build_announce_sdp(1, "10.0.0.2".parse().unwrap(), "10.0.0.9".parse().unwrap(), &cipher);
    assert!(sdp.contains("a=rsaaeskey:"));
    assert!(sdp.contains("a=aesiv:"));
}

#[test]
fn transport_parses_the_receiver_ports() {
    let t = parse_transport(
        "RTP/AVP/UDP;unicast;mode=record;server_port=6000;control_port=6001;timing_port=6002",
    )
    .unwrap();
    assert_eq!(t.server_port, 6000);
    assert_eq!(t.control_port, 6001);
    assert_eq!(t.timing_port, 6002);
    // A response missing a port is not a valid transport.
    assert!(parse_transport("RTP/AVP/UDP;server_port=6000").is_none());
}

/// A scripted fake receiver drives the handshake and asserts the §7.1 method
/// order, the ANNOUNCE SDP, and that SET_PARAMETER carries the volume — no
/// UDP, no real device.
#[test]
fn handshake_drives_the_7_1_sequence_in_order() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let mut methods = Vec::new();
        let mut announce_sdp = String::new();
        let mut volume_body = String::new();

        for _ in 0..5 {
            let (method, cseq, body) = read_request(&mut reader);
            match method.as_str() {
                "ANNOUNCE" => announce_sdp = body,
                "SET_PARAMETER" => volume_body = body,
                _ => {}
            }
            let extra = if method == "SETUP" {
                "Session: ABCD1234\r\nTransport: RTP/AVP/UDP;unicast;server_port=6000;\
                 control_port=6001;timing_port=6002\r\n"
            } else if method == "RECORD" {
                // The receiver reports its audio latency in frames.
                "Audio-Latency: 11025\r\n"
            } else {
                ""
            };
            writer
                .write_all(format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n{extra}\r\n").as_bytes())
                .unwrap();
            writer.flush().unwrap();
            methods.push(method);
        }
        (methods, announce_sdp, volume_body)
    });

    let mut conn = RtspConnection::connect(addr.ip(), addr.port(), "bae/test").unwrap();
    let sdp = build_announce_sdp(7, conn.local_addr(), addr.ip(), &RaopCipher::none());
    let (negotiated, audio_latency) = perform_handshake(
        &mut conn,
        "rtsp://x/7",
        &sdp,
        6100,
        6200,
        0,
        0,
        bae_playback::Volume::new(0.5).unwrap(),
    )
    .unwrap();

    assert_eq!(negotiated.server_port, 6000);
    assert_eq!(negotiated.control_port, 6001);
    assert_eq!(negotiated.timing_port, 6002);
    assert_eq!(
        audio_latency,
        Some(11_025),
        "the receiver's reported Audio-Latency is parsed and returned"
    );

    let (methods, announce_sdp, volume_body) = server.join().unwrap();
    assert_eq!(methods, vec!["OPTIONS", "ANNOUNCE", "SETUP", "RECORD", "SET_PARAMETER"]);
    assert!(announce_sdp.contains("a=rtpmap:96 L16/44100/2"));
    assert!(volume_body.contains("volume: -15."), "0.5 volume maps to -15 dB, got {volume_body:?}");
}

/// A source with nothing to send: the session runs, no audio flows.
struct SilentSource;
impl PcmSource for SilentSource {
    fn next_frames(&mut self, _out: &mut [i16]) -> usize {
        0
    }
}

/// A whole RAOP session against a scripted loopback receiver that reports
/// `Audio-Latency: 11025` on RECORD: playback is told the latency the
/// session paces by, the receiver's, not the ~2 s default.
#[test]
fn playback_is_told_the_latency_the_receiver_reports() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    // The receiver's audio and control ports, bound so the stream's sends land.
    let audio = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let control = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let (audio_port, control_port) =
        (audio.local_addr().unwrap().port(), control.local_addr().unwrap().port());

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        // Answer every request until the sender closes the connection.
        loop {
            let (method, cseq, _body) = read_request(&mut reader);
            if method.is_empty() {
                break;
            }
            let extra = match method.as_str() {
                "SETUP" => format!(
                    "Transport: RTP/AVP/UDP;unicast;server_port={audio_port};\
                     control_port={control_port};timing_port=1\r\n"
                ),
                "RECORD" => "Audio-Latency: 11025\r\n".to_string(),
                _ => String::new(),
            };
            writer
                .write_all(format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n{extra}\r\n").as_bytes())
                .unwrap();
            writer.flush().unwrap();
        }
    });

    let session = RaopSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        addr.ip(),
        addr.port(),
        Box::new(SilentSource),
        RaopCipher::none(),
        bae_playback::Volume::FULL,
        Arc::new(crate::airplay::stream::SystemClock::new()),
    )
    .expect("the RAOP session starts");

    let paced = session.control.stream.latency_frames();
    let session: Arc<dyn AirPlayStreamControl> = Arc::new(session);
    let told = session.latency();
    drop(session);
    server.join().unwrap();

    assert_eq!(paced, 11_025, "the session paces by the reported latency");
    assert_eq!(
        told,
        Duration::from_millis(250),
        "playback offsets by the latency the session paces by"
    );
}

/// Read one RTSP request from the fake receiver's socket: the method, the
/// CSeq, and the body (drained by Content-Length). A closed connection
/// reads as an empty method.
fn read_request(reader: &mut impl BufRead) -> (String, String, String) {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let method = line.split(' ').next().unwrap_or("").to_string();

    let mut cseq = String::new();
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((key, value)) = header.split_once(':') {
            match key.trim().to_ascii_lowercase().as_str() {
                "cseq" => cseq = value.trim().to_string(),
                "content-length" => {
                    content_length = value.trim().parse().expect("valid content-length")
                }
                _ => {}
            }
        }
    }
    let mut body = vec![0u8; content_length];
    std::io::Read::read_exact(reader, &mut body).unwrap();
    (method, cseq, String::from_utf8_lossy(&body).into_owned())
}
