//! The AirPlay 2 control sequence over the encrypted RTSP channel.
//!
//! After transient pair-setup establishes the shared secret, an AirPlay 2
//! sender drives its control messages over the HomeKit secure transport
//! ([`super::ap2_channel::HapChannel`]): SETUP (session), SETUP (audio stream,
//! carrying the `shk` audio key), SETPEERS (the timing-peer list), then RECORD.
//! The bodies are binary plists ([`super::bplist`]); the message shapes follow
//! the MIT `airplay2-rs` and pyatv senders.
//!
//! The session keeps time over NTP, as pyatv always does and OwnTone does
//! without its PTP daemon: bae runs no PTP clock, and a Mac's AirPlay Receiver
//! refuses a SETUP that announces one nobody answers. Its realtime stream is
//! anchored by sync packets on the control port, as both senders anchor one,
//! not by SETRATEANCHORTIME, which a Mac's receiver refuses for it; pause is a
//! FLUSH, as for RAOP.
//!
//! The bodies are pure functions (golden-tested), and the whole sequence is
//! driven over a real encrypted socket against a scripted fake receiver that
//! decrypts with the pair-setup key and checks the order and contents.

use std::io::{BufReader, Read, Write};
use std::net::{IpAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bae_playback::airplay_output::{AirPlayFailed, PCM_CHANNELS};
use tracing::warn;

use super::airplay2::Ap2AudioCipher;
use super::ap2_channel::{audio_key, HapChannel};
use super::ap2_pairing::{run_pair_verify, run_transient_pair_setup};
use super::bplist::{self, Plist};
use super::digest::{DigestAnswer, DigestPassword, DigestStyle, Unauthorized};
use super::pairing::PairingError;
use super::pairing_keys::KeptPairing;
use super::rtsp::{
    Method, RtspConnection, RtspRequest, RtspResponse, SenderIdentity, UNAUTHORIZED,
};
use super::stream::{
    poisoned_connection, MonotonicClock, PayloadCrypto, PcmFeed, RaopStream, RaopStreamControl,
    RtpAudio, RtpPosition, StreamEndpoints, StreamFailure, StreamSockets, DEFAULT_LATENCY_FRAMES,
};

const BPLIST_CONTENT_TYPE: &str = "application/x-apple-binary-plist";

/// The `User-Agent` of an AirPlay 2 session's connections. HomePod and tvOS
/// 27 answer 403 to a request whose `User-Agent` is not an AirPlay sender's
/// (OwnTone issue 2042: `AirPlay/380.20.1` and `iTunes/12` pass, `AirPlay/1.0`
/// and `owntone/29.3` do not), so bae sends the one OwnTone sends by default
/// since that issue, which its reporters confirmed on OS 27 receivers. pyatv
/// sends `AirPlay/550.10`.
pub(crate) const AP2_USER_AGENT: &str = "AirPlay/999.0.0";

/// This device, named `client_name`, as an AirPlay 2 sender.
pub(crate) fn ap2_sender(client_name: &str) -> SenderIdentity {
    SenderIdentity::new(AP2_USER_AGENT, client_name)
}

/// The SETUP session (step 1) body: NTP timing at bae's timing port, the
/// session identity the receiver keys its session on, and this device's
/// `name`, which the receiver shows its owner (pyatv sends it there, and
/// shairport-sync reads the sender's name from it).
pub(crate) fn setup_session_body(
    group_uuid: &str,
    session_uuid: &str,
    mac_address: &str,
    timing_port: u16,
    name: &str,
) -> Plist {
    Plist::Dict(vec![
        ("name".to_string(), Plist::String(name.to_string())),
        ("timingProtocol".to_string(), Plist::String("NTP".to_string())),
        ("groupUUID".to_string(), Plist::String(group_uuid.to_string())),
        ("sessionUUID".to_string(), Plist::String(session_uuid.to_string())),
        ("macAddress".to_string(), Plist::String(mac_address.to_string())),
        ("timingPort".to_string(), Plist::Integer(u64::from(timing_port))),
    ])
}

/// The SETUP audio-stream (step 2) body: one realtime stream with the audio key
/// the receiver decrypts packets with, the sender's control/timing ports, and
/// the latency window.
pub(crate) fn setup_stream_body(shk: &[u8; 32], control_port: u16, timing_port: u16) -> Plist {
    let stream = Plist::Dict(vec![
        // 96 = realtime audio (RTP payload type 0x60).
        ("type".to_string(), Plist::Integer(96)),
        // Compression type 1 = the uncompressed L16 stream bae sends.
        ("ct".to_string(), Plist::Integer(1)),
        ("spf".to_string(), Plist::Integer(u64::from(super::stream::FRAMES_PER_PACKET))),
        ("shk".to_string(), Plist::Data(shk.to_vec())),
        ("controlPort".to_string(), Plist::Integer(u64::from(control_port))),
        ("timingPort".to_string(), Plist::Integer(u64::from(timing_port))),
        ("latencyMin".to_string(), Plist::Integer(11_025)),
        ("latencyMax".to_string(), Plist::Integer(88_200)),
    ]);
    Plist::Dict(vec![("streams".to_string(), Plist::Array(vec![stream]))])
}

/// The SETPEERS body: the timing peers as an array of IP-address strings.
pub(crate) fn set_peers_body(peers: &[IpAddr]) -> Plist {
    Plist::Array(peers.iter().map(|ip| Plist::String(ip.to_string())).collect())
}

/// The receiver's ports and reported latency parsed from a SETUP audio-stream
/// response plist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ap2StreamPorts {
    /// Where audio datagrams go (`dataPort`).
    pub data_port: u16,
    /// Where sync datagrams go (`controlPort`).
    pub control_port: u16,
    /// The receiver's `audioLatency` in frames, when reported.
    pub audio_latency_frames: Option<u32>,
}

/// Pull the receiver's `dataPort`/`controlPort` (and `audioLatency`, if present)
/// out of a SETUP streams response.
pub(crate) fn parse_stream_ports(response: &Plist) -> Option<Ap2StreamPorts> {
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
#[derive(Debug, thiserror::Error)]
pub(crate) enum Ap2Error {
    #[error("AirPlay 2 control I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Channel(#[from] super::ap2_channel::ChannelError),
    #[error("receiver rejected {step} (status {status})")]
    Rejected { step: &'static str, status: u16 },
    #[error("AirPlay 2 response body: {0}")]
    BadBody(&'static str),
    #[error("AirPlay 2 response body is not a readable bplist: {0}")]
    BodyDecode(#[from] bplist::BplistError),
    /// Pair-setup or pair-verify failed.
    #[error("AirPlay 2 pairing failed: {0}")]
    Pairing(#[from] PairingError),
}

/// The AirPlay 2 control connection after pair-setup: RTSP requests sealed onto
/// the encrypted transport, responses decrypted back. Built from the shared
/// secret pair-setup produced.
pub(crate) struct Ap2Control {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    channel: HapChannel,
    cseq: u32,
    uri: String,
    /// Who sends every request.
    sender: SenderIdentity,
    /// The password a receiver that has one asks for again, as HTTP Digest,
    /// after pair-verify, and the challenge that asked for it.
    answer: Option<DigestAnswer>,
}

impl Ap2Control {
    /// Wrap an already-verified stream in the encrypted transport. `uri` is the
    /// RTSP request URI (`rtsp://<local>/<session-id>`). `password` answers a
    /// receiver that asks for it as HTTP Digest.
    pub(crate) fn new(
        stream: TcpStream,
        shared_secret: &[u8],
        uri: impl Into<String>,
        sender: SenderIdentity,
        password: Option<String>,
    ) -> std::io::Result<Self> {
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Ap2Control {
            writer: stream,
            reader,
            channel: HapChannel::from_shared_secret(shared_secret),
            cseq: 0,
            uri: uri.into(),
            sender,
            answer: password.map(|password| {
                DigestAnswer::new(DigestPassword { password, style: DigestStyle::Standard })
            }),
        })
    }

    /// Send one RTSP request over the encrypted channel and read its response.
    pub(crate) fn request(&mut self, request: &RtspRequest) -> Result<RtspResponse, Ap2Error> {
        self.cseq += 1;
        let mut extra = self.sender.headers();
        let answer = self.answer.as_ref();
        if let Some(authorization) =
            answer.and_then(|answer| answer.authorization(request.method.as_str(), &request.uri))
        {
            extra.push(("Authorization".to_string(), authorization));
        }
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

    /// The control sequence: SETUP (session), SETUP (audio stream), SETPEERS,
    /// RECORD — returning the receiver's audio/control ports.
    pub(crate) fn run_setup_sequence(
        &mut self,
        shk: &[u8; 32],
        session: &Ap2SetupParams,
    ) -> Result<Ap2StreamPorts, Ap2Error> {
        self.plist_request(
            Method::Setup,
            &setup_session_body(
                &session.group_uuid,
                &session.session_uuid,
                &session.mac_address,
                session.local_timing_port,
                self.sender.client_name(),
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

        self.request_expecting(&RtspRequest::new(Method::Record, self.uri.clone()), "RECORD")?;
        Ok(ports)
    }

    /// FLUSH the receiver's buffer (pause / pre-seek) up to `next`, the
    /// packet the sender sends next, as OwnTone pauses an AirPlay 2 receiver.
    /// A Mac's AirPlay Receiver refuses a FLUSH whose `RTP-Info` names none.
    pub(crate) fn flush(&mut self, next: RtpPosition) -> Result<(), Ap2Error> {
        self.request_expecting(
            &RtspRequest::new(Method::Flush, self.uri.clone()).header("RTP-Info", next.rtp_info()),
            "FLUSH",
        )?;
        Ok(())
    }

    /// TEARDOWN the session over the encrypted channel.
    pub(crate) fn teardown(&mut self) -> Result<(), Ap2Error> {
        self.request_expecting(&RtspRequest::new(Method::Teardown, self.uri.clone()), "TEARDOWN")?;
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
        Ok(bplist::decode(&response.body)?)
    }

    /// Send a request and require a 2xx status, answering a Digest challenge
    /// with the password when the receiver asks for it.
    fn request_expecting(
        &mut self,
        request: &RtspRequest,
        step: &'static str,
    ) -> Result<RtspResponse, Ap2Error> {
        let mut response = self.request(request)?;
        if response.status == UNAUTHORIZED {
            response = self.answer_challenge(request, response)?;
        }
        if response.is_success() {
            Ok(response)
        } else {
            Err(Ap2Error::Rejected { step, status: response.status })
        }
    }

    /// Send `request` again with the password, after the receiver answered
    /// it with `refused`, a 401. Without a password the 401 stands. A
    /// receiver that refuses the password, on this request or after it took
    /// it once, no longer accepts what bae kept for it.
    fn answer_challenge(
        &mut self,
        request: &RtspRequest,
        refused: RtspResponse,
    ) -> Result<RtspResponse, Ap2Error> {
        let Some(answer) = &mut self.answer else {
            return Ok(refused);
        };
        match answer.unauthorized(refused.header("WWW-Authenticate")) {
            Unauthorized::Refused => return Err(PairingError::PairingLost.into()),
            Unauthorized::Unreadable => {
                return Err(Ap2Error::BadBody("a 401 without a Digest challenge"));
            }
            Unauthorized::Challenged => {}
        }
        let answered = self.request(request)?;
        if answered.status == UNAUTHORIZED {
            return Err(PairingError::PairingLost.into());
        }
        Ok(answered)
    }
}

/// The session-identity and port parameters the AirPlay 2 setup sequence carries.
pub(crate) struct Ap2SetupParams {
    pub group_uuid: String,
    pub session_uuid: String,
    pub mac_address: String,
    pub local_control_port: u16,
    pub local_timing_port: u16,
    pub peers: Vec<IpAddr>,
}

/// How each AirPlay 2 session proves bae to its receiver, which gives the
/// secret the session's control channel and audio are keyed from.
#[derive(Debug, Clone)]
pub(crate) enum Ap2Auth {
    /// Transient pair-setup: the receiver asks for no code, and nothing is
    /// kept.
    Transient,
    /// Pair-verify with what a pairing by code kept.
    Verify(KeptPairing),
}

impl Ap2Auth {
    /// Pair on `conn` and return the secret the session is keyed from: the
    /// SRP key of a transient pair-setup, or the X25519 secret of a
    /// pair-verify.
    fn session_secret(&self, conn: &mut RtspConnection) -> Result<Vec<u8>, Ap2Error> {
        match self {
            Ap2Auth::Transient => run_transient_pair_setup(conn),
            Ap2Auth::Verify(kept) => Ok(run_pair_verify(conn, &kept.keys)?.to_vec()),
        }
    }

    /// The password the receiver asks for again as HTTP Digest, if it has one.
    fn password(&self) -> Option<String> {
        match self {
            Ap2Auth::Transient => None,
            Ap2Auth::Verify(kept) => kept.password.clone(),
        }
    }
}

/// Pair with the receiver once, as `auth` says, on a connection of its own,
/// which closes when this returns: what a cast learns about the receiver
/// before playback moves. Every session start pairs again on its own
/// connection. `client_name` names this device to the receiver.
pub(crate) fn pair_once(
    receiver: IpAddr,
    airplay_port: u16,
    client_name: &str,
    auth: &Ap2Auth,
) -> Result<(), Ap2Error> {
    let mut conn = RtspConnection::connect(receiver, airplay_port, ap2_sender(client_name))?;
    auth.session_secret(&mut conn)?;
    Ok(())
}

/// A cloneable handle to a running AirPlay 2 session's controls, matching the
/// RAOP one's shape: pause is an encrypted FLUSH, resume re-anchors the
/// sender's pacing, and teardown is an encrypted TEARDOWN. The encrypted
/// control connection is behind a mutex because only these methods touch it.
#[derive(Clone)]
struct Ap2SessionControl {
    control: Arc<Mutex<Ap2Control>>,
    stream: RaopStreamControl,
}

impl Ap2SessionControl {
    /// FLUSH the receiver's buffer (pause / pre-seek) up to the packet the
    /// stream sends next.
    fn flush(&self) -> Result<(), Ap2Error> {
        let next = self.stream.next_packet();
        self.connection()?.flush(next)
    }

    /// Re-anchor the sender's pacing (resume / post-seek): the audio thread
    /// restarts its burst and re-marks the stream, and the sync packet right
    /// after the burst anchors the receiver again.
    fn reanchor(&self) {
        self.stream.reanchor();
    }

    /// TEARDOWN the session on the receiver.
    fn teardown(&self) -> Result<(), Ap2Error> {
        self.connection()?.teardown()
    }

    /// The encrypted control connection. One a panic left mid-request can't
    /// be read in step with its replies again, so it takes no more requests.
    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Ap2Control>, Ap2Error> {
        self.control.lock().map_err(|poisoned| Ap2Error::Io(poisoned_connection(poisoned)))
    }
}

/// A live AirPlay 2 session: the encrypted control connection and the running
/// audio stream. Dropping it tears the receiver down and stops the threads.
pub(crate) struct Ap2Session {
    control: Ap2SessionControl,
    _stream: RaopStream,
}

impl Ap2Session {
    /// Connect to an AirPlay 2 receiver, pair as `auth` says naming this
    /// device `client_name`, drive the encrypted SETUP/SETPEERS/RECORD
    /// sequence while answering the receiver's timing requests, and start
    /// streaming `feed` as ChaCha-encrypted audio.
    pub(crate) fn start(
        diagnostics: &bae_diagnostics::Diagnostics,
        receiver: IpAddr,
        airplay_port: u16,
        client_name: &str,
        auth: &Ap2Auth,
        feed: PcmFeed,
        failed: AirPlayFailed,
        clock: Arc<dyn MonotonicClock>,
        ids: &dyn coven::IdSource,
    ) -> Result<Self, Ap2Error> {
        let sender = ap2_sender(client_name);
        let mut conn = RtspConnection::connect(receiver, airplay_port, sender.clone())?;
        let local_ip = conn.local_addr();

        let shared = auth.session_secret(&mut conn)?;

        // Open the timing/control sockets before SETUP so their ports are
        // announced. bae answers timing requests from now on: the receiver
        // syncs its clock before it answers SETUP (stream).
        let failure = StreamFailure::new(failed);
        let sockets = StreamSockets::open(diagnostics, &failure)?;
        let local_timing_port = sockets.timing_port();
        let local_control_port = sockets.control_port()?;

        let shk = audio_key(&shared);
        let session_uuid = ids.new_id().to_string();
        let group_uuid = ids.new_id().to_string();
        let uri = format!("rtsp://{local_ip}/{session_uuid}");

        let mut control =
            Ap2Control::new(conn.into_stream(), &shared, uri, sender, auth.password())?;
        let params = Ap2SetupParams {
            group_uuid,
            session_uuid,
            mac_address: local_mac_address(),
            local_control_port,
            local_timing_port,
            peers: vec![local_ip, receiver],
        };
        let ports = control.run_setup_sequence(&shk, &params)?;

        // The receiver's reported latency, else the dialect's default.
        let latency = ports.audio_latency_frames.unwrap_or(DEFAULT_LATENCY_FRAMES);
        let endpoints = StreamEndpoints {
            receiver,
            audio_port: ports.data_port,
            control_port: ports.control_port,
        };
        let cipher = Ap2AudioCipher::from_shared_secret(&shared);
        let stream_control = RaopStreamControl::new(latency, clock, failure);
        let stream = RaopStream::spawn(
            diagnostics,
            feed,
            PayloadCrypto::Ap2(cipher),
            endpoints,
            RtpAudio { ssrc: rand::random::<u32>(), channels: PCM_CHANNELS },
            sockets,
            stream_control.clone(),
        )?;

        let session_control =
            Ap2SessionControl { control: Arc::new(Mutex::new(control)), stream: stream_control };
        Ok(Ap2Session { control: session_control, _stream: stream })
    }

    pub(crate) fn flush(&self) -> Result<(), Ap2Error> {
        self.control.flush()
    }

    pub(crate) fn reanchor(&self) {
        self.control.reanchor();
    }

    /// The receiver latency the stream paces by, settled by the SETUP sequence.
    pub(crate) fn latency(&self) -> Duration {
        self.control.stream.latency()
    }

    /// The audio the stream has sent that the receiver has not played yet.
    pub(crate) fn unheard(&self) -> Duration {
        self.control.stream.unheard()
    }

    /// Why the audio flow to the receiver failed for good (a dead
    /// receiver), when it has, so the session should be ended.
    pub(crate) fn failure(&self) -> Option<String> {
        self.control.stream.failure()
    }
}

impl Drop for Ap2Session {
    fn drop(&mut self) {
        if let Err(error) = self.control.teardown() {
            warn!("airplay 2 receiver teardown failed: {error}");
        }
    }
}

/// A locally-administered MAC-shaped identifier for the SETUP session body. Real
/// receivers don't authenticate it in a transient session; it's random per run.
fn local_mac_address() -> String {
    let b: [u8; 6] = rand::random();
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[0] | 0x02, b[1], b[2], b[3], b[4], b[5])
}

#[cfg(test)]
#[path = "ap2_session_tests.rs"]
mod tests;
