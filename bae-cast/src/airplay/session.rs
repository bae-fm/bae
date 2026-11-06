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

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bae_playback::airplay_output::{AirPlayFailed, PCM_CHANNELS, PCM_SAMPLE_RATE};
use tracing::warn;

use super::crypto::RaopCipher;
use super::rtp::NtpTime;
use super::rtsp::{Method, RtspConnection, RtspRequest, RtspResponse};
use super::stream::{
    poisoned_connection, MonotonicClock, PayloadCrypto, PcmFeed, RaopStream, RaopStreamControl,
    RtpAudio, StreamEndpoints, StreamSockets, DEFAULT_LATENCY_FRAMES, FRAMES_PER_PACKET,
};

/// A failure setting up or running a RAOP session.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RaopError {
    /// The control connection failed at the socket level.
    #[error("RAOP connection error: {0}")]
    Io(#[from] std::io::Error),
    /// A receiver returned a non-2xx status for a handshake step.
    #[error("receiver rejected {step} (status {status})")]
    Rejected { step: &'static str, status: u16 },
    /// A SETUP response was missing the ports the sender needs.
    #[error("SETUP response did not carry the receiver's ports")]
    MissingTransport,
}

/// The receiver's UDP ports, parsed from a SETUP response's `Transport` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NegotiatedTransport {
    /// Where audio datagrams go (`server_port`).
    pub server_port: u16,
    /// Where sync datagrams go (`control_port`).
    pub control_port: u16,
    /// The receiver's timing port (`timing_port`).
    pub timing_port: u16,
}

/// Map a volume to the RAOP dB level SET_PARAMETER carries: silence is the
/// sentinel −144, otherwise a linear map onto the −30 dB … 0 dB range the
/// receiver applies.
pub(crate) fn volume_to_raop_db(level: bae_playback::Volume) -> f32 {
    let level = level.get();
    if level <= 0.0 {
        -144.0
    } else {
        -30.0 + level * 30.0
    }
}

/// Build the ANNOUNCE SDP body for an `L16` PCM stream. The `rsaaeskey`/`aesiv`
/// lines appear only for an encrypted stream.
pub(crate) fn build_announce_sdp(
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
         a=rtpmap:96 L16/{PCM_SAMPLE_RATE}/{PCM_CHANNELS}\r\n\
         a=fmtp:96 {FRAMES_PER_PACKET} 0 16 40 10 14 2 255 0 0 {PCM_SAMPLE_RATE}\r\n"
    );
    if let (Some(key), Some(iv)) = (cipher.rsaaeskey_b64(), cipher.aesiv_b64()) {
        sdp.push_str(&format!("a=rsaaeskey:{key}\r\n"));
        sdp.push_str(&format!("a=aesiv:{iv}\r\n"));
    }
    sdp
}

/// Parse the receiver's ports from a SETUP `Transport` header
/// (`RTP/AVP/UDP;unicast;server_port=6000;control_port=6001;timing_port=6002`).
pub(crate) fn parse_transport(header: &str) -> Option<NegotiatedTransport> {
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
    Some(NegotiatedTransport { server_port: server?, control_port: control?, timing_port: timing? })
}

fn expect_success(step: &'static str, response: &RtspResponse) -> Result<(), RaopError> {
    if response.is_success() {
        Ok(())
    } else {
        Err(RaopError::Rejected { step, status: response.status })
    }
}

/// The receiver's `Audio-Latency` (in frames), when it reports one on a SETUP or
/// RECORD response — the ~2 s buffer the sender paces ahead by and offsets the
/// position back by.
fn parse_audio_latency(response: &RtspResponse) -> Option<u32> {
    response.header("Audio-Latency").and_then(|v| v.trim().parse().ok())
}

/// Drive the §7.1 RTSP sequence to arm a receiver: OPTIONS, ANNOUNCE the SDP,
/// SETUP announcing the sender's `control_port`/`timing_port`, RECORD with the
/// RTP anchor, and an initial volume SET_PARAMETER. Returns the receiver's ports
/// and the audio latency it reported, if any.
pub(crate) fn perform_handshake(
    conn: &mut RtspConnection,
    uri: &str,
    sdp: &str,
    local_control_port: u16,
    local_timing_port: u16,
    initial_sequence: u16,
    initial_timestamp: u32,
    volume: bae_playback::Volume,
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
    let negotiated =
        setup.header("Transport").and_then(parse_transport).ok_or(RaopError::MissingTransport)?;

    let record = conn.request(
        &RtspRequest::new(Method::Record, uri)
            .header("Range", "npt=0-")
            .header("RTP-Info", format!("seq={initial_sequence};rtptime={initial_timestamp}")),
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
struct RaopControl {
    rtsp: Arc<Mutex<RtspConnection>>,
    uri: String,
    stream: RaopStreamControl,
}

impl RaopControl {
    /// FLUSH the receiver's buffer (pause / pre-seek).
    fn flush(&self) -> Result<(), RaopError> {
        self.rtsp_request(Method::Flush, "FLUSH")
    }

    /// Re-anchor the pacing (resume / post-seek): the audio thread restarts its
    /// lead and re-marks the stream.
    fn reanchor(&self) {
        self.stream.reanchor();
    }

    /// TEARDOWN the session on the receiver.
    fn teardown(&self) -> Result<(), RaopError> {
        self.rtsp_request(Method::Teardown, "TEARDOWN")
    }

    fn rtsp_request(&self, method: Method, step: &'static str) -> Result<(), RaopError> {
        let mut rtsp = self.rtsp.lock().map_err(poisoned_connection)?;
        let response = rtsp.request(&RtspRequest::new(method, &self.uri))?;
        expect_success(step, &response)
    }
}

/// A live RAOP session: the control connection and the running audio stream.
/// Dropping it tears the receiver down and stops the stream threads.
pub(crate) struct RaopSession {
    control: RaopControl,
    _stream: RaopStream,
}

impl RaopSession {
    /// Connect to `receiver`, run the handshake, and start streaming `feed`.
    /// `cipher` is the negotiated encryption ([`RaopCipher::none`] for `et=0`).
    pub(crate) fn start(
        diagnostics: &bae_diagnostics::Diagnostics,
        receiver: IpAddr,
        rtsp_port: u16,
        feed: PcmFeed,
        failed: AirPlayFailed,
        cipher: RaopCipher,
        volume: bae_playback::Volume,
        clock: Arc<dyn MonotonicClock>,
    ) -> Result<Self, RaopError> {
        let mut conn = RtspConnection::connect(receiver, rtsp_port, "bae/1 (RAOP)")?;
        let local_ip = conn.local_addr();

        // Bind the timing and control sockets before SETUP so their ports can be
        // announced: the receiver sends timing requests to the former, and sync
        // packets leave from the latter.
        let sockets = StreamSockets::bind()?;
        let local_timing_port = sockets.timing_port()?;
        let local_control_port = sockets.control_port()?;

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

        // The receiver's reported latency, else the buffer AirPort Express expects.
        let latency = reported_latency.unwrap_or(DEFAULT_LATENCY_FRAMES);
        let endpoints = StreamEndpoints {
            receiver,
            audio_port: negotiated.server_port,
            control_port: negotiated.control_port,
        };
        let stream_control = RaopStreamControl::new(latency, failed);
        let stream = RaopStream::spawn(
            diagnostics,
            feed,
            PayloadCrypto::Raop(cipher),
            endpoints,
            RtpAudio {
                ssrc,
                sample_rate: PCM_SAMPLE_RATE,
                channels: PCM_CHANNELS,
                initial_timestamp,
            },
            sockets,
            clock,
            true, // RAOP sends periodic sync packets
            stream_control.clone(),
        )?;

        let control = RaopControl { rtsp: Arc::new(Mutex::new(conn)), uri, stream: stream_control };
        Ok(RaopSession { control, _stream: stream })
    }

    pub(crate) fn flush(&self) -> Result<(), RaopError> {
        self.control.flush()
    }

    pub(crate) fn reanchor(&self) {
        self.control.reanchor();
    }

    /// The receiver latency the stream paces by, settled by the handshake.
    pub(crate) fn latency(&self) -> Duration {
        self.control.stream.latency()
    }

    /// Why the audio flow to the receiver failed for good (a dead
    /// receiver), when it has, so the session should be ended.
    pub(crate) fn failure(&self) -> Option<String> {
        self.control.stream.failure()
    }
}

impl Drop for RaopSession {
    fn drop(&mut self) {
        // Best-effort receiver teardown; the stream threads stop when the
        // `RaopStream` field drops.
        if let Err(error) = self.control.teardown() {
            warn!("airplay receiver teardown failed: {error}");
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
