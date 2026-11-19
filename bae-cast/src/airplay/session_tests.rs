use super::*;
use crate::airplay::crypto::apple_public_key;
use crate::airplay::digest::DigestStyle;
use crate::airplay::fake_raop_receiver::{Challenges, DigestGate};
use crate::airplay::fake_verification_receiver::{listen_for_verification, VerificationReceiver};
use crate::airplay::rtsp::read_request;
use bae_playback::airplay_output::{AirPlayStreamControl, PcmSource};
use std::io::{BufReader, Write};
use std::net::{TcpListener, UdpSocket};

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

    let server = bae_util::test_spawn::thread(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let mut methods = Vec::new();
        let mut announce_sdp = String::new();
        let mut volume_body = String::new();

        for _ in 0..5 {
            let request = read_request(&mut reader).unwrap().expect("the sender sent a request");
            let cseq = request.cseq().to_string();
            match request.method {
                Method::Announce => announce_sdp = request.body_text(),
                Method::SetParameter => volume_body = request.body_text(),
                _ => {}
            }
            let extra = if request.method == Method::Setup {
                "Session: ABCD1234\r\nTransport: RTP/AVP/UDP;unicast;server_port=6000;\
                 control_port=6001;timing_port=6002\r\n"
            } else if request.method == Method::Record {
                // The receiver reports its audio latency in frames.
                "Audio-Latency: 11025\r\n"
            } else {
                ""
            };
            writer
                .write_all(format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n{extra}\r\n").as_bytes())
                .unwrap();
            writer.flush().unwrap();
            methods.push(request.method.as_str().to_string());
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

    let server = bae_util::test_spawn::thread(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        // Answer every request until the sender closes the connection.
        while let Some(request) =
            read_request(&mut reader).unwrap_or_else(|error| panic!("fake receiver: {error}"))
        {
            let cseq = request.cseq().to_string();
            let extra = match request.method {
                Method::Setup => format!(
                    "Transport: RTP/AVP/UDP;unicast;server_port={audio_port};\
                     control_port={control_port};timing_port=1\r\n"
                ),
                Method::Record => "Audio-Latency: 11025\r\n".to_string(),
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
        None,
        None,
        crate::airplay::stream::PcmFeed::new(
            Box::new(SilentSource),
            bae_playback::airplay_output::PcmWake::unattached(),
        ),
        bae_playback::airplay_output::AirPlayFailed::ignored(),
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

/// A receiver that knows `key`, answering OPTIONS with 403 on a connection
/// that didn't verify, with its RTP ports bound so the stream's sends land.
fn verifying_receiver(
    key: &crate::airplay::pairing_keys::VerificationKey,
) -> (u16, Arc<std::sync::Mutex<VerificationReceiver>>, [UdpSocket; 2]) {
    let audio = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let control = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let mut receiver = VerificationReceiver::new(&[]);
    receiver.paired = vec![crate::airplay::pairing::public_key(&key.seed)];
    receiver.forbids_unverified = true;
    receiver.ports = (audio.local_addr().unwrap().port(), control.local_addr().unwrap().port());
    let (port, receiver) = listen_for_verification(receiver);
    (port, receiver, [audio, control])
}

fn verification(
    key: &crate::airplay::pairing_keys::VerificationKey,
) -> crate::airplay::device_verification::SessionVerification {
    crate::airplay::device_verification::SessionVerification {
        key: key.clone(),
        client_name: "Studio Mac".to_string(),
    }
}

/// A session to a receiver that verifies the device proves bae's key on its
/// connection before the handshake, which the receiver then takes.
#[test]
fn a_session_verifies_the_device_before_its_handshake() {
    let key = crate::airplay::pairing_keys::VerificationKey::new();
    let (port, receiver, _rtp) = verifying_receiver(&key);
    let session = RaopSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        [127, 0, 0, 1].into(),
        port,
        Some(&verification(&key)),
        None,
        crate::airplay::stream::PcmFeed::new(
            Box::new(SilentSource),
            bae_playback::airplay_output::PcmWake::unattached(),
        ),
        bae_playback::airplay_output::AirPlayFailed::ignored(),
        RaopCipher::none(),
        bae_playback::Volume::FULL,
        Arc::new(crate::airplay::stream::SystemClock::new()),
    )
    .expect("the verified session starts");
    drop(session);

    let requests = receiver.lock().unwrap().requests.clone();
    assert_eq!(
        requests[..3],
        ["POST /pair-verify", "POST /pair-verify", "OPTIONS *"],
        "{requests:?}"
    );
}

/// A session start on a receiver that no longer knows bae's key fails as a
/// lost pairing, naming the receiver, so playback can say which one.
#[test]
fn a_session_start_on_a_receiver_that_forgot_the_key_names_it() {
    use bae_playback::airplay_output::AirPlaySink;
    let key = crate::airplay::pairing_keys::VerificationKey::new();
    let (port, receiver, _rtp) = verifying_receiver(&key);
    receiver.lock().unwrap().forget_paired();
    let device = bae_playback::renderer::CastReceiver {
        id: "001122334455".to_string(),
        name: "Studio".to_string(),
        kind: bae_playback::renderer::RendererKind::AirPlay,
        model: None,
        address: "127.0.0.1".to_string(),
        takes_volume: true,
    };
    let sink = crate::airplay::sink::RaopSink::new(
        [127, 0, 0, 1].into(),
        port,
        Some(verification(&key)),
        None,
        crate::airplay::capabilities::RaopEncryption::None,
        bae_playback::Volume::FULL,
        device.clone(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let Err(error) = sink.start(
        Box::new(SilentSource),
        bae_playback::airplay_output::PcmWake::unattached(),
        bae_playback::airplay_output::AirPlayFailed::ignored(),
    ) else {
        panic!("a receiver that forgot the key starts no session");
    };
    assert!(
        matches!(&error, bae_playback::AudioError::AirPlayPairingLost(receiver) if *receiver == device),
        "{error}"
    );
}

/// A receiver with the password `hunter2` asked for as `challenges` says, with
/// its RTP ports bound so the stream's sends land.
fn password_receiver(
    challenges: Challenges,
) -> (u16, Arc<std::sync::Mutex<VerificationReceiver>>, [UdpSocket; 2]) {
    let audio = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let control = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let mut receiver = VerificationReceiver::new(&[]);
    receiver.password = Some(DigestGate::new("hunter2", DigestStyle::Standard, challenges));
    receiver.ports = (audio.local_addr().unwrap().port(), control.local_addr().unwrap().port());
    let (port, receiver) = listen_for_verification(receiver);
    (port, receiver, [audio, control])
}

fn password(password: &str) -> RaopPassword {
    RaopPassword { password: password.to_string(), style: DigestStyle::Standard }
}

fn start_session(port: u16, password: &RaopPassword) -> Result<RaopSession, RaopError> {
    RaopSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        [127, 0, 0, 1].into(),
        port,
        None,
        Some(password),
        crate::airplay::stream::PcmFeed::new(
            Box::new(SilentSource),
            bae_playback::airplay_output::PcmWake::unattached(),
        ),
        bae_playback::airplay_output::AirPlayFailed::ignored(),
        RaopCipher::none(),
        bae_playback::Volume::FULL,
        Arc::new(crate::airplay::stream::SystemClock::new()),
    )
}

/// A session with the password answers the receiver's first 401 and carries
/// the answer on every later request, its TEARDOWN included.
#[test]
fn a_session_answers_every_request_with_the_password() {
    let (port, receiver, _rtp) = password_receiver(Challenges::EveryRequest);
    let session = start_session(port, &password("hunter2")).expect("the session starts");
    drop(session);
    let receiver = receiver.lock().unwrap();
    let taken = &receiver.password.as_ref().unwrap().taken;
    let methods: Vec<_> = taken.iter().map(|t| (t.method.as_str(), t.status)).collect();
    assert_eq!(
        methods,
        [
            ("OPTIONS", 401),
            ("OPTIONS", 200),
            ("ANNOUNCE", 200),
            ("SETUP", 200),
            ("RECORD", 200),
            ("SET_PARAMETER", 200),
            ("TEARDOWN", 200),
        ]
    );
    assert!(taken[1..].iter().all(|t| t.authorized));
}

/// A receiver that refuses the password with a second 401 fails the session
/// start, and the sink says the receiver no longer accepts this device.
#[test]
fn a_session_start_the_receiver_refuses_the_password_of_names_the_receiver() {
    use bae_playback::airplay_output::AirPlaySink;
    let (port, _receiver, _rtp) = password_receiver(Challenges::EveryRequest);
    let error = start_session(port, &password("hunter3")).err().expect("the receiver refuses");
    assert!(matches!(error, RaopError::PasswordRefused), "{error:?}");

    let device = bae_playback::renderer::CastReceiver {
        id: "001122334455".to_string(),
        name: "Studio".to_string(),
        kind: bae_playback::renderer::RendererKind::AirPlay,
        model: None,
        address: "127.0.0.1".to_string(),
        takes_volume: true,
    };
    let sink = crate::airplay::sink::RaopSink::new(
        [127, 0, 0, 1].into(),
        port,
        None,
        Some(password("hunter3")),
        crate::airplay::capabilities::RaopEncryption::None,
        bae_playback::Volume::FULL,
        device.clone(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let Err(error) = sink.start(
        Box::new(SilentSource),
        bae_playback::airplay_output::PcmWake::unattached(),
        bae_playback::airplay_output::AirPlayFailed::ignored(),
    ) else {
        panic!("a receiver that refuses the password starts no session");
    };
    assert!(
        matches!(&error, bae_playback::AudioError::AirPlayPairingLost(receiver) if *receiver == device),
        "{error}"
    );
}
