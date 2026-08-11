//! The AirPlay 2 control sequence over the encrypted RTSP channel.
//!
//! After pair-setup and pair-verify establish the shared secret, an AirPlay 2
//! sender drives its control messages over the HomeKit secure transport
//! ([`super::ap2_channel::HapChannel`]): SETUP (session), SETUP (audio stream,
//! carrying the `shk` audio key), SETPEERS (the timing-peer list), and
//! SETRATEANCHORTIME (the RTP-timeline anchor), then RECORD. The bodies are
//! binary plists ([`super::bplist`]); the message shapes follow the MIT
//! `airplay2-rs` and pyatv senders.
//!
//! The bodies are pure functions (golden-tested), and the whole sequence is
//! driven over a real encrypted socket against a scripted fake receiver that
//! decrypts with the pair-verify key and checks the order and contents.

use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};

use super::airplay2::{Ap2AudioCipher, PairVerify};
use super::ap2_channel::{audio_key, HapChannel};
use super::bplist::{self, Plist};
use super::pairing::{PairingError, TransientPairing};
use super::rtp::NtpTime;
use super::rtsp::{Method, RtspConnection, RtspRequest, RtspResponse};
use super::session::{CHANNELS, SAMPLE_RATE};
use super::stream::{
    MonotonicClock, PayloadCrypto, PcmSource, RaopStream, RaopStreamControl, StreamEndpoints,
};

/// The AirPlay 2 audio latency in frames when a receiver doesn't report one.
const DEFAULT_LATENCY_FRAMES: u32 = 88_200;

const BPLIST_CONTENT_TYPE: &str = "application/x-apple-binary-plist";

/// The SETUP session (step 1) body: the timing protocol and session identity the
/// receiver keys its session on. NTP mode omits the PTP `timingPeerInfo`.
pub fn setup_session_body(
    timing_protocol: &str,
    group_uuid: &str,
    session_uuid: &str,
    mac_address: &str,
    timing_port: u16,
    local_ip: IpAddr,
) -> Plist {
    let mut entries = vec![
        (
            "timingProtocol".to_string(),
            Plist::String(timing_protocol.to_string()),
        ),
        (
            "groupUUID".to_string(),
            Plist::String(group_uuid.to_string()),
        ),
        (
            "sessionUUID".to_string(),
            Plist::String(session_uuid.to_string()),
        ),
        (
            "macAddress".to_string(),
            Plist::String(mac_address.to_string()),
        ),
        (
            "timingPort".to_string(),
            Plist::Integer(u64::from(timing_port)),
        ),
    ];
    if timing_protocol == "PTP" {
        // The receiver listens for our PTP clock on these addresses.
        entries.push((
            "timingPeerInfo".to_string(),
            Plist::Dict(vec![(
                "Addresses".to_string(),
                Plist::Array(vec![Plist::String(local_ip.to_string())]),
            )]),
        ));
    }
    Plist::Dict(entries)
}

/// The SETUP audio-stream (step 2) body: one realtime stream with the audio key
/// the receiver decrypts packets with, the sender's control/timing ports, and
/// the latency window.
pub fn setup_stream_body(shk: &[u8; 32], control_port: u16, timing_port: u16) -> Plist {
    let stream = Plist::Dict(vec![
        // 96 = realtime audio (RTP payload type 0x60).
        ("type".to_string(), Plist::Integer(96)),
        // Compression type 1 = the uncompressed L16 stream bae sends.
        ("ct".to_string(), Plist::Integer(1)),
        (
            "spf".to_string(),
            Plist::Integer(u64::from(super::stream::FRAMES_PER_PACKET)),
        ),
        ("shk".to_string(), Plist::Data(shk.to_vec())),
        (
            "controlPort".to_string(),
            Plist::Integer(u64::from(control_port)),
        ),
        (
            "timingPort".to_string(),
            Plist::Integer(u64::from(timing_port)),
        ),
        ("latencyMin".to_string(), Plist::Integer(11_025)),
        ("latencyMax".to_string(), Plist::Integer(88_200)),
    ]);
    Plist::Dict(vec![("streams".to_string(), Plist::Array(vec![stream]))])
}

/// The SETPEERS body: the timing peers as an array of IP-address strings.
pub fn set_peers_body(peers: &[IpAddr]) -> Plist {
    Plist::Array(
        peers
            .iter()
            .map(|ip| Plist::String(ip.to_string()))
            .collect(),
    )
}

/// The SETRATEANCHORTIME body: `rate` 1 to play or 0 to pause, the RTP timestamp
/// the anchor pins, and — for PTP timing — the network-time anchor it maps to.
pub fn set_rate_anchor_body(rate: u8, rtp_time: u32, ptp_anchor: Option<(u64, u32, u64)>) -> Plist {
    let mut entries = vec![
        ("rate".to_string(), Plist::Integer(u64::from(rate))),
        ("rtpTime".to_string(), Plist::Integer(u64::from(rtp_time))),
    ];
    if let Some((secs, frac, timeline_id)) = ptp_anchor {
        entries.push(("networkTimeSecs".to_string(), Plist::Integer(secs)));
        entries.push((
            "networkTimeFrac".to_string(),
            Plist::Integer(u64::from(frac)),
        ));
        entries.push((
            "networkTimeTimelineID".to_string(),
            Plist::Integer(timeline_id),
        ));
    }
    Plist::Dict(entries)
}

/// The receiver's ports and reported latency parsed from a SETUP audio-stream
/// response plist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ap2StreamPorts {
    /// Where audio datagrams go (`dataPort`).
    pub data_port: u16,
    /// Where sync datagrams go (`controlPort`).
    pub control_port: u16,
    /// The receiver's `audioLatency` in frames, when reported.
    pub audio_latency_frames: Option<u32>,
}

/// Pull the receiver's `dataPort`/`controlPort` (and `audioLatency`, if present)
/// out of a SETUP streams response.
pub fn parse_stream_ports(response: &Plist) -> Option<Ap2StreamPorts> {
    let stream = match response.get("streams")? {
        Plist::Array(items) => items.first()?,
        _ => return None,
    };
    Some(Ap2StreamPorts {
        data_port: stream.get("dataPort")?.as_integer()? as u16,
        control_port: stream.get("controlPort")?.as_integer()? as u16,
        audio_latency_frames: stream
            .get("audioLatency")
            .and_then(Plist::as_integer)
            .map(|v| v as u32),
    })
}

/// A failure driving the AirPlay 2 control channel.
#[derive(Debug)]
pub enum Ap2Error {
    Io(std::io::Error),
    Channel(super::ap2_channel::ChannelError),
    Rejected {
        step: &'static str,
        status: u16,
    },
    BadBody(&'static str),
    /// Transient pair-setup or pair-verify failed.
    Pairing(PairingError),
}

impl std::fmt::Display for Ap2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ap2Error::Io(e) => write!(f, "AirPlay 2 control I/O error: {e}"),
            Ap2Error::Channel(e) => write!(f, "{e}"),
            Ap2Error::Rejected { step, status } => {
                write!(f, "receiver rejected {step} (status {status})")
            }
            Ap2Error::BadBody(what) => write!(f, "AirPlay 2 response body: {what}"),
            Ap2Error::Pairing(e) => write!(f, "AirPlay 2 pairing failed: {e}"),
        }
    }
}

impl std::error::Error for Ap2Error {}

impl From<PairingError> for Ap2Error {
    fn from(e: PairingError) -> Self {
        Ap2Error::Pairing(e)
    }
}

impl From<std::io::Error> for Ap2Error {
    fn from(e: std::io::Error) -> Self {
        Ap2Error::Io(e)
    }
}

impl From<super::ap2_channel::ChannelError> for Ap2Error {
    fn from(e: super::ap2_channel::ChannelError) -> Self {
        Ap2Error::Channel(e)
    }
}

/// The AirPlay 2 control connection after pair-verify: RTSP requests sealed onto
/// the encrypted transport, responses decrypted back. Built from the shared
/// secret pair-verify produced.
pub struct Ap2Control {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    channel: HapChannel,
    cseq: u32,
    uri: String,
    user_agent: String,
}

impl Ap2Control {
    /// Wrap an already-verified stream in the encrypted transport. `uri` is the
    /// RTSP request URI (`rtsp://<local>/<session-id>`).
    pub fn new(
        stream: TcpStream,
        shared_secret: &[u8; 32],
        uri: impl Into<String>,
        user_agent: impl Into<String>,
    ) -> std::io::Result<Self> {
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Ap2Control {
            writer: stream,
            reader,
            channel: HapChannel::from_shared_secret(shared_secret),
            cseq: 0,
            uri: uri.into(),
            user_agent: user_agent.into(),
        })
    }

    /// The audio session key `shk` the SETUP streams body carries.
    pub fn audio_key(shared_secret: &[u8; 32]) -> [u8; 32] {
        audio_key(shared_secret)
    }

    /// Send one RTSP request over the encrypted channel and read its response.
    pub fn request(&mut self, request: &RtspRequest) -> Result<RtspResponse, Ap2Error> {
        self.cseq += 1;
        let extra = vec![("User-Agent".to_string(), self.user_agent.clone())];
        let plaintext = request.serialize(self.cseq, &extra);
        let sealed = self.channel.seal(&plaintext);
        self.writer.write_all(&sealed)?;
        self.writer.flush()?;
        self.read_response()
    }

    /// Read and decrypt one full RTSP response.
    fn read_response(&mut self) -> Result<RtspResponse, Ap2Error> {
        let mut plaintext = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            // Try to parse a complete response from what's decrypted so far.
            let mut cursor = std::io::Cursor::new(&plaintext);
            match RtspResponse::read_from(&mut cursor) {
                Ok(response) => return Ok(response),
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                Err(e) => return Err(Ap2Error::Io(e)),
            }
            // Need more bytes: read, decrypt every whole block available.
            let read = self.reader.read(&mut chunk)?;
            if read == 0 {
                return Err(Ap2Error::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "encrypted control channel closed mid-response",
                )));
            }
            self.channel.feed(&chunk[..read]);
            while let Some(block) = self.channel.next_block()? {
                plaintext.extend_from_slice(&block);
            }
        }
    }

    /// The §control sequence a HomePod requires: SETUP (session), SETUP (audio
    /// stream), SETPEERS, SETRATEANCHORTIME, RECORD — returning the receiver's
    /// audio/control ports.
    pub fn run_setup_sequence(
        &mut self,
        shk: &[u8; 32],
        session: &Ap2SetupParams,
    ) -> Result<Ap2StreamPorts, Ap2Error> {
        self.plist_request(
            Method::Setup,
            &setup_session_body(
                &session.timing_protocol,
                &session.group_uuid,
                &session.session_uuid,
                &session.mac_address,
                session.local_timing_port,
                session.local_ip,
            ),
            "SETUP(session)",
        )?;

        let stream_response = self.plist_request(
            Method::Setup,
            &setup_stream_body(shk, session.local_control_port, session.local_timing_port),
            "SETUP(stream)",
        )?;
        let ports = parse_stream_ports(&stream_response)
            .ok_or(Ap2Error::BadBody("SETUP stream response lacked ports"))?;

        // SETPEERS: our address and the receiver's, as timing peers.
        self.request_expecting(
            &RtspRequest::with_body(
                Method::SetPeers,
                self.uri.clone(),
                "/peer-list-changed",
                bplist::encode(&set_peers_body(&session.peers)),
            ),
            "SETPEERS",
        )?;

        // SETRATEANCHORTIME: anchor the RTP timeline to the network clock.
        self.request_expecting(
            &RtspRequest::with_body(
                Method::SetRateAnchorTime,
                self.uri.clone(),
                BPLIST_CONTENT_TYPE,
                bplist::encode(&set_rate_anchor_body(1, 0, session.ptp_anchor)),
            ),
            "SETRATEANCHORTIME",
        )?;

        self.request_expecting(
            &RtspRequest::new(Method::Record, self.uri.clone()),
            "RECORD",
        )?;
        Ok(ports)
    }

    /// Change the playback rate with a fresh SETRATEANCHORTIME — `0` pauses the
    /// receiver's rendering, `1` resumes it. AirPlay 2's equivalent of RAOP's
    /// FLUSH / re-anchor.
    pub fn set_rate(&mut self, rate: u8) -> Result<(), Ap2Error> {
        self.request_expecting(
            &RtspRequest::with_body(
                Method::SetRateAnchorTime,
                self.uri.clone(),
                BPLIST_CONTENT_TYPE,
                bplist::encode(&set_rate_anchor_body(rate, 0, None)),
            ),
            "SETRATEANCHORTIME",
        )?;
        Ok(())
    }

    /// TEARDOWN the session over the encrypted channel.
    pub fn teardown(&mut self) -> Result<(), Ap2Error> {
        self.request_expecting(
            &RtspRequest::new(Method::Teardown, self.uri.clone()),
            "TEARDOWN",
        )?;
        Ok(())
    }

    /// Send a plist-bodied request and decode the response body as a plist.
    fn plist_request(
        &mut self,
        method: Method,
        body: &Plist,
        step: &'static str,
    ) -> Result<Plist, Ap2Error> {
        let request = RtspRequest::with_body(
            method,
            self.uri.clone(),
            BPLIST_CONTENT_TYPE,
            bplist::encode(body),
        );
        let response = self.request_expecting(&request, step)?;
        bplist::decode(&response.body).map_err(|_| Ap2Error::BadBody("response was not a bplist"))
    }

    /// Send a request and require a 2xx status.
    fn request_expecting(
        &mut self,
        request: &RtspRequest,
        step: &'static str,
    ) -> Result<RtspResponse, Ap2Error> {
        let response = self.request(request)?;
        if response.is_success() {
            Ok(response)
        } else {
            Err(Ap2Error::Rejected {
                step,
                status: response.status,
            })
        }
    }
}

/// The session-identity and port parameters the AirPlay 2 setup sequence carries.
pub struct Ap2SetupParams {
    pub timing_protocol: String,
    pub group_uuid: String,
    pub session_uuid: String,
    pub mac_address: String,
    pub local_ip: IpAddr,
    pub local_control_port: u16,
    pub local_timing_port: u16,
    pub peers: Vec<IpAddr>,
    /// The PTP anchor (network seconds, fraction, timeline id) when timing is PTP.
    pub ptp_anchor: Option<(u64, u32, u64)>,
}

/// Drive transient pair-setup (SRP) over the plaintext RTSP socket. The SRP
/// shared secret proves transient pairing; the channel keys come from the
/// pair-verify that follows.
fn run_pair_setup(conn: &mut RtspConnection) -> Result<(), Ap2Error> {
    let mut pairing = TransientPairing::new();
    let m1 = pairing.start()?;
    let m2 = pair_post(conn, "/pair-setup", m1)?;
    let m3 = pairing.handle_m2(&m2)?;
    let m4 = pair_post(conn, "/pair-setup", m3)?;
    pairing.handle_m4(&m4)?;
    Ok(())
}

/// Drive pair-verify (X25519 + Ed25519) over the plaintext RTSP socket, returning
/// the shared secret the encrypted channel and audio stream are keyed from.
fn run_pair_verify(conn: &mut RtspConnection) -> Result<[u8; 32], Ap2Error> {
    let mut verify = PairVerify::new();
    let m1 = verify.start()?;
    let m2 = pair_post(conn, "/pair-verify", m1)?;
    let m3 = verify.handle_m2(&m2)?;
    let m4 = pair_post(conn, "/pair-verify", m3)?;
    Ok(verify.handle_m4(&m4)?)
}

/// POST a TLV8 pairing body and return the response body, or an error on a
/// non-2xx status.
fn pair_post(conn: &mut RtspConnection, path: &str, body: Vec<u8>) -> Result<Vec<u8>, Ap2Error> {
    let response = conn.request(&RtspRequest::with_body(
        Method::Post,
        path,
        "application/octet-stream",
        body,
    ))?;
    if response.is_success() {
        Ok(response.body)
    } else {
        Err(Ap2Error::Rejected {
            step: "pairing",
            status: response.status,
        })
    }
}

/// A cloneable handle to a running AirPlay 2 session's controls, matching the
/// RAOP one's shape: pause (rate 0) / resume (rate 1 + re-anchor) map to
/// SETRATEANCHORTIME, teardown to an encrypted TEARDOWN. The encrypted control
/// connection is behind a mutex because only these methods touch it.
#[derive(Clone)]
struct Ap2SessionControl {
    control: Arc<Mutex<Ap2Control>>,
    stream: RaopStreamControl,
    latency_frames: u32,
}

impl Ap2SessionControl {
    /// Pause the receiver's rendering (rate 0) — AirPlay 2's FLUSH.
    fn flush(&self) -> Result<(), Ap2Error> {
        self.control.lock().unwrap().set_rate(0)
    }

    /// Resume rendering (rate 1) and re-anchor the sender's pacing.
    fn reanchor(&self) -> Result<(), Ap2Error> {
        self.stream.reanchor();
        self.control.lock().unwrap().set_rate(1)
    }

    /// TEARDOWN the session on the receiver.
    fn teardown(&self) -> Result<(), Ap2Error> {
        self.control.lock().unwrap().teardown()
    }

    fn frames_sent(&self) -> u64 {
        self.stream.frames_sent()
    }

    /// Whether the audio flow to the receiver has failed persistently.
    fn has_failed(&self) -> bool {
        self.stream.has_failed()
    }

    fn latency_frames(&self) -> u32 {
        self.latency_frames
    }
}

/// A live AirPlay 2 session: the encrypted control connection and the running
/// audio stream. Dropping it tears the receiver down and stops the threads.
pub struct Ap2Session {
    control: Ap2SessionControl,
    _stream: RaopStream,
}

impl Ap2Session {
    /// Connect to an AirPlay 2 receiver, run transient pair-setup then pair-verify,
    /// drive the encrypted SETUP/SETPEERS/SETRATEANCHORTIME/RECORD sequence, and
    /// start streaming `source` as ChaCha-encrypted audio.
    pub fn start(
        receiver: IpAddr,
        airplay_port: u16,
        source: Box<dyn PcmSource>,
        latency_frames: Option<u32>,
        timing: super::airplay2::TimingProtocol,
        clock: Arc<dyn MonotonicClock>,
    ) -> Result<Self, Ap2Error> {
        use super::airplay2::TimingProtocol;

        let mut conn = RtspConnection::connect(receiver, airplay_port, "bae/1 (AirPlay 2)")?;
        let local_ip = conn.local_addr();

        run_pair_setup(&mut conn)?;
        let shared = run_pair_verify(&mut conn)?;

        // Bind the timing/control sockets before SETUP so their ports are announced.
        let timing_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let control_socket = UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0))?;
        let local_timing_port = timing_socket.local_addr()?.port();
        let local_control_port = control_socket.local_addr()?.port();

        let shk = audio_key(&shared);
        let session_uuid = uuid::Uuid::new_v4().to_string();
        let group_uuid = uuid::Uuid::new_v4().to_string();
        let uri = format!("rtsp://{local_ip}/{session_uuid}");

        // A receiver that requires PTP is told so, and its SETRATEANCHORTIME carries
        // the network-time anchor; NTP receivers use the RAOP-style timing responder
        // and no anchor. The decision is the one `TimingProtocol::from_features` made.
        let (timing_protocol, ptp_anchor) = match timing {
            TimingProtocol::Ptp => {
                let now = NtpTime::now();
                let timeline_id = rand::random::<u64>();
                (
                    "PTP".to_string(),
                    Some((u64::from(now.seconds), now.fraction, timeline_id)),
                )
            }
            TimingProtocol::Ntp => ("NTP".to_string(), None),
        };

        let mut control = Ap2Control::new(conn.into_stream(), &shared, uri, "bae/1 (AirPlay 2)")?;
        let params = Ap2SetupParams {
            timing_protocol,
            group_uuid,
            session_uuid,
            mac_address: local_mac_address(),
            local_ip,
            local_control_port,
            local_timing_port,
            peers: vec![local_ip, receiver],
            ptp_anchor,
        };
        let ports = control.run_setup_sequence(&shk, &params)?;

        // The receiver's reported latency wins; the caller's value is the fallback.
        let latency = ports
            .audio_latency_frames
            .or(latency_frames)
            .unwrap_or(DEFAULT_LATENCY_FRAMES);
        let endpoints = StreamEndpoints {
            receiver,
            audio_port: ports.data_port,
            control_port: ports.control_port,
            latency_frames: latency,
        };
        let cipher = Ap2AudioCipher::from_shared_secret(&shared);
        let stream_control = RaopStreamControl::new();
        let stream = RaopStream::spawn(
            source,
            PayloadCrypto::Ap2(cipher),
            endpoints,
            rand::random::<u32>(),
            SAMPLE_RATE,
            CHANNELS,
            0,
            timing_socket,
            control_socket,
            clock,
            false, // AirPlay 2 anchors with SETRATEANCHORTIME, not RAOP sync packets
            stream_control.clone(),
        )?;

        let session_control = Ap2SessionControl {
            control: Arc::new(Mutex::new(control)),
            stream: stream_control,
            latency_frames: latency,
        };
        Ok(Ap2Session {
            control: session_control,
            _stream: stream,
        })
    }

    pub fn flush(&self) -> Result<(), Ap2Error> {
        self.control.flush()
    }

    pub fn reanchor(&self) -> Result<(), Ap2Error> {
        self.control.reanchor()
    }

    pub fn has_failed(&self) -> bool {
        self.control.has_failed()
    }

    pub fn frames_sent(&self) -> u64 {
        self.control.frames_sent()
    }

    pub fn latency_frames(&self) -> u32 {
        self.control.latency_frames()
    }
}

impl Drop for Ap2Session {
    fn drop(&mut self) {
        let _ = self.control.teardown();
    }
}

/// A locally-administered MAC-shaped identifier for the SETUP session body. Real
/// receivers don't authenticate it in a transient session; it's random per run.
fn local_mac_address() -> String {
    let b: [u8; 6] = rand::random();
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        b[0] | 0x02,
        b[1],
        b[2],
        b[3],
        b[4],
        b[5]
    )
}

#[cfg(test)]
#[path = "ap2_session_tests.rs"]
mod tests;
