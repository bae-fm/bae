//! The RAOP session: the §7.1 RTSP handshake that arms a receiver, then the live
//! audio stream.
//!
//! A RAOP session drives one control connection through the fixed sequence
//! OPTIONS → ANNOUNCE → SETUP → RECORD → SET_PARAMETER (openairplay spec §7.1),
//! negotiating the receiver's audio/control/timing ports along the way, then
//! spawns the [`RaopStream`] that pushes audio. Volume rides SET_PARAMETER as a
//! dB level; FLUSH and TEARDOWN end or interrupt playback.
//!
//! The handshake is factored out of the socket plumbing so it is driven, in
//! order, against a scripted fake receiver with no UDP and no real device.

use std::net::{IpAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::crypto::RaopCipher;
use super::rtp::NtpTime;
use super::rtsp::{Method, RtspConnection, RtspRequest, RtspResponse};
use super::stream::{
    MonotonicClock, PayloadCrypto, PcmSource, RaopStream, StreamEndpoints, FRAMES_PER_PACKET,
};

/// RAOP streams 44.1 kHz / 16-bit / stereo — bae's pipeline resamples to it.
pub const SAMPLE_RATE: u32 = 44_100;
pub const CHANNELS: u32 = 2;

/// The RAOP audio latency in frames when a receiver doesn't report one — the
/// ~2 s buffer AirPort Express expects.
const DEFAULT_LATENCY_FRAMES: u32 = 88_200;

/// A failure setting up or running a RAOP session.
#[derive(Debug)]
pub enum RaopError {
    /// The control connection failed at the socket level.
    Io(std::io::Error),
    /// A receiver returned a non-2xx status for a handshake step.
    Rejected { step: &'static str, status: u16 },
    /// A SETUP response was missing the ports the sender needs.
    MissingTransport,
}

impl std::fmt::Display for RaopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RaopError::Io(e) => write!(f, "RAOP connection error: {e}"),
            RaopError::Rejected { step, status } => {
                write!(f, "receiver rejected {step} (status {status})")
            }
            RaopError::MissingTransport => {
                write!(f, "SETUP response did not carry the receiver's ports")
            }
        }
    }
}

impl std::error::Error for RaopError {}

impl From<std::io::Error> for RaopError {
    fn from(e: std::io::Error) -> Self {
        RaopError::Io(e)
    }
}

/// The receiver's UDP ports, parsed from a SETUP response's `Transport` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedTransport {
    /// Where audio datagrams go (`server_port`).
    pub server_port: u16,
    /// Where sync datagrams go (`control_port`).
    pub control_port: u16,
    /// The receiver's timing port (`timing_port`).
    pub timing_port: u16,
}

/// Map a 0.0–1.0 volume to the RAOP dB level SET_PARAMETER carries: silence is
/// the sentinel −144, otherwise a linear map onto the −30 dB … 0 dB range the
/// receiver applies.
pub fn volume_to_raop_db(level: f32) -> f32 {
    let level = level.clamp(0.0, 1.0);
    if level <= 0.0 {
        -144.0
    } else {
        -30.0 + level * 30.0
    }
}

/// Build the ANNOUNCE SDP body for an `L16` PCM stream. The `rsaaeskey`/`aesiv`
/// lines appear only for an encrypted stream.
pub fn build_announce_sdp(
    session_id: u32,
    local_ip: IpAddr,
    receiver_ip: IpAddr,
    cipher: &RaopCipher,
) -> String {
    let mut sdp = format!(
        "v=0\r\n\
         o=iTunes {session_id} 0 IN IP4 {local_ip}\r\n\
         s=iTunes\r\n\
         c=IN IP4 {receiver_ip}\r\n\
         t=0 0\r\n\
         m=audio 0 RTP/AVP 96\r\n\
         a=rtpmap:96 L16/{SAMPLE_RATE}/{CHANNELS}\r\n\
         a=fmtp:96 {FRAMES_PER_PACKET} 0 16 40 10 14 2 255 0 0 {SAMPLE_RATE}\r\n"
    );
    if let (Some(key), Some(iv)) = (cipher.rsaaeskey_b64(), cipher.aesiv_b64()) {
        sdp.push_str(&format!("a=rsaaeskey:{key}\r\n"));
        sdp.push_str(&format!("a=aesiv:{iv}\r\n"));
    }
    sdp
}

/// Parse the receiver's ports from a SETUP `Transport` header
/// (`RTP/AVP/UDP;unicast;server_port=6000;control_port=6001;timing_port=6002`).
pub fn parse_transport(header: &str) -> Option<NegotiatedTransport> {
    let mut server = None;
    let mut control = None;
    let mut timing = None;
    for field in header.split(';') {
        if let Some((key, value)) = field.split_once('=') {
            let port = value.trim().parse().ok();
            match key.trim() {
                "server_port" => server = port,
                "control_port" => control = port,
                "timing_port" => timing = port,
                _ => {}
            }
        }
    }
    Some(NegotiatedTransport {
        server_port: server?,
        control_port: control?,
        timing_port: timing?,
    })
}

fn expect_success(step: &'static str, response: &RtspResponse) -> Result<(), RaopError> {
    if response.is_success() {
        Ok(())
    } else {
        Err(RaopError::Rejected {
            step,
            status: response.status,
        })
    }
}

/// The receiver's `Audio-Latency` (in frames), when it reports one on a SETUP or
/// RECORD response — the ~2 s buffer the sender paces ahead by and offsets the
/// position back by.
fn parse_audio_latency(response: &RtspResponse) -> Option<u32> {
    response
        .header("Audio-Latency")
        .and_then(|v| v.trim().parse().ok())
}

/// Drive the §7.1 RTSP sequence to arm a receiver: OPTIONS, ANNOUNCE the SDP,
/// SETUP announcing the sender's `control_port`/`timing_port`, RECORD with the
/// RTP anchor, and an initial volume SET_PARAMETER. Returns the receiver's ports
/// and the audio latency it reported, if any.
#[allow(clippy::too_many_arguments)]
pub fn perform_handshake(
    conn: &mut RtspConnection,
    uri: &str,
    sdp: &str,
    local_control_port: u16,
    local_timing_port: u16,
    initial_sequence: u16,
    initial_timestamp: u32,
    volume: f32,
) -> Result<(NegotiatedTransport, Option<u32>), RaopError> {
    let options = conn.request(&RtspRequest::new(Method::Options, "*"))?;
    expect_success("OPTIONS", &options)?;

    let announce = conn.request(&RtspRequest::with_body(
        Method::Announce,
        uri,
        "application/sdp",
        sdp.as_bytes().to_vec(),
    ))?;
    expect_success("ANNOUNCE", &announce)?;

    let transport = format!(
        "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;\
         control_port={local_control_port};timing_port={local_timing_port}"
    );
    let setup =
        conn.request(&RtspRequest::new(Method::Setup, uri).header("Transport", transport))?;
    expect_success("SETUP", &setup)?;
    let negotiated = setup
        .header("Transport")
        .and_then(parse_transport)
        .ok_or(RaopError::MissingTransport)?;

    let record = conn.request(
        &RtspRequest::new(Method::Record, uri)
            .header("Range", "npt=0-")
            .header(
                "RTP-Info",
                format!("seq={initial_sequence};rtptime={initial_timestamp}"),
            ),
    )?;
    expect_success("RECORD", &record)?;
    // The receiver reports its audio latency on SETUP or RECORD; RECORD wins.
    let audio_latency = parse_audio_latency(&record).or_else(|| parse_audio_latency(&setup));

    let volume_body = format!("volume: {:.6}\r\n", volume_to_raop_db(volume));
    let set_volume = conn.request(&RtspRequest::with_body(
        Method::SetParameter,
        uri,
        "text/parameters",
        volume_body.into_bytes(),
    ))?;
    expect_success("SET_PARAMETER", &set_volume)?;

    Ok((negotiated, audio_latency))
}

/// A cloneable handle to a running RAOP session's transport controls: FLUSH,
/// re-anchor, volume, and teardown, plus the audible-position counter. The audio
/// output holds the session while the playback service drives it through one of
/// these from its own thread — the RTSP control connection is guarded by a mutex
/// because only these methods (never the UDP threads) touch it.
#[derive(Clone)]
pub struct RaopControl {
    rtsp: Arc<Mutex<RtspConnection>>,
    uri: String,
    reanchor: Arc<AtomicU64>,
    frames_sent: Arc<AtomicU64>,
    failed: Arc<std::sync::atomic::AtomicBool>,
    latency_frames: u32,
}

impl RaopControl {
    /// FLUSH the receiver's buffer (pause / pre-seek).
    pub fn flush(&self) -> Result<(), RaopError> {
        self.rtsp_request(Method::Flush, "FLUSH")
    }

    /// Re-anchor the pacing (resume / post-seek): the audio thread restarts its
    /// lead and re-marks the stream.
    pub fn reanchor(&self) {
        self.reanchor.fetch_add(1, Ordering::Release);
    }

    /// Set the receiver volume (0.0–1.0) via SET_PARAMETER.
    pub fn set_volume(&self, level: f32) -> Result<(), RaopError> {
        let body = format!("volume: {:.6}\r\n", volume_to_raop_db(level));
        let response = self.rtsp.lock().unwrap().request(&RtspRequest::with_body(
            Method::SetParameter,
            &self.uri,
            "text/parameters",
            body.into_bytes(),
        ))?;
        expect_success("SET_PARAMETER", &response)
    }

    /// TEARDOWN the session on the receiver.
    pub fn teardown(&self) -> Result<(), RaopError> {
        self.rtsp_request(Method::Teardown, "TEARDOWN")
    }

    /// Frames handed to the receiver so far.
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    /// Whether the audio flow to the receiver has failed persistently (a dead
    /// receiver), so the session should be ended.
    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// The receiver's audio latency in frames — the offset between frames sent and
    /// what is audible, for the position the UI shows.
    pub fn latency_frames(&self) -> u32 {
        self.latency_frames
    }

    fn rtsp_request(&self, method: Method, step: &'static str) -> Result<(), RaopError> {
        let response = self
            .rtsp
            .lock()
            .unwrap()
            .request(&RtspRequest::new(method, &self.uri))?;
        expect_success(step, &response)
    }
}

/// A live RAOP session: the control connection and the running audio stream.
/// Dropping it tears the receiver down and stops the stream threads.
pub struct RaopSession {
    control: RaopControl,
    _stream: RaopStream,
}

impl RaopSession {
    /// Connect to `receiver`, run the handshake, and start streaming `source`.
    /// `cipher` is the negotiated encryption ([`RaopCipher::none`] for `et=0`).
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        receiver: IpAddr,
        rtsp_port: u16,
        source: Box<dyn PcmSource>,
        cipher: RaopCipher,
        latency_frames: Option<u32>,
        volume: f32,
        clock: Arc<dyn MonotonicClock>,
    ) -> Result<Self, RaopError> {
        let mut conn = RtspConnection::connect(receiver, rtsp_port, "bae/1 (RAOP)")?;
        let local_ip = conn.local_addr();

        // Bind the timing and control sockets before SETUP so their ports can be
        // announced: the receiver sends timing requests to the former, and sync
        // packets leave from the latter.
        let timing_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let control_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let local_timing_port = timing_socket.local_addr()?.port();
        let local_control_port = control_socket.local_addr()?.port();

        let session_id = NtpTime::now().fraction;
        let ssrc = NtpTime::now().seconds;
        let uri = format!("rtsp://{local_ip}/{session_id}");
        let sdp = build_announce_sdp(session_id, local_ip, receiver, &cipher);

        let initial_timestamp = 0u32;
        let (negotiated, reported_latency) = perform_handshake(
            &mut conn,
            &uri,
            &sdp,
            local_control_port,
            local_timing_port,
            0,
            initial_timestamp,
            volume,
        )?;

        // The receiver's reported latency wins; the caller's value is the fallback,
        // and the constant only the last resort when neither is known.
        let latency = reported_latency
            .or(latency_frames)
            .unwrap_or(DEFAULT_LATENCY_FRAMES);
        let endpoints = StreamEndpoints {
            receiver,
            audio_port: negotiated.server_port,
            control_port: negotiated.control_port,
            latency_frames: latency,
        };
        let stream = RaopStream::spawn(
            source,
            PayloadCrypto::Raop(cipher),
            endpoints,
            ssrc,
            SAMPLE_RATE,
            CHANNELS,
            initial_timestamp,
            timing_socket,
            control_socket,
            clock,
            true, // RAOP sends periodic sync packets
        )?;

        let control = RaopControl {
            rtsp: Arc::new(Mutex::new(conn)),
            uri,
            reanchor: stream.reanchor_handle(),
            frames_sent: stream.frames_sent_handle(),
            failed: stream.failed_handle(),
            latency_frames: latency,
        };
        Ok(RaopSession {
            control,
            _stream: stream,
        })
    }

    /// A cloneable handle to this session's transport controls, for the playback
    /// service to drive pause/resume/seek/volume/teardown and read position.
    pub fn control(&self) -> RaopControl {
        self.control.clone()
    }
}

impl Drop for RaopSession {
    fn drop(&mut self) {
        // Best-effort receiver teardown; the stream threads stop when the
        // `RaopStream` field drops.
        let _ = self.control.teardown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airplay::crypto::apple_public_key;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    #[test]
    fn volume_maps_to_raop_db_range() {
        assert_eq!(volume_to_raop_db(1.0), 0.0);
        assert_eq!(volume_to_raop_db(0.5), -15.0);
        assert_eq!(volume_to_raop_db(0.0), -144.0);
        // Out-of-range is clamped.
        assert_eq!(volume_to_raop_db(2.0), 0.0);
        assert_eq!(volume_to_raop_db(-1.0), -144.0);
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
        let sdp = build_announce_sdp(
            1,
            "10.0.0.2".parse().unwrap(),
            "10.0.0.9".parse().unwrap(),
            &cipher,
        );
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
        let (negotiated, audio_latency) =
            perform_handshake(&mut conn, "rtsp://x/7", &sdp, 6100, 6200, 0, 0, 0.5).unwrap();

        assert_eq!(negotiated.server_port, 6000);
        assert_eq!(negotiated.control_port, 6001);
        assert_eq!(negotiated.timing_port, 6002);
        assert_eq!(
            audio_latency,
            Some(11_025),
            "the receiver's reported Audio-Latency is parsed and returned"
        );

        let (methods, announce_sdp, volume_body) = server.join().unwrap();
        assert_eq!(
            methods,
            vec!["OPTIONS", "ANNOUNCE", "SETUP", "RECORD", "SET_PARAMETER"]
        );
        assert!(announce_sdp.contains("a=rtpmap:96 L16/44100/2"));
        assert!(
            volume_body.contains("volume: -15."),
            "0.5 volume maps to -15 dB, got {volume_body:?}"
        );
    }

    /// Read one RTSP request from the fake receiver's socket: the method, the
    /// CSeq, and the body (drained by Content-Length).
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
}
