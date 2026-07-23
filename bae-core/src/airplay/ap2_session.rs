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
use std::net::{IpAddr, TcpStream};

use super::ap2_channel::{audio_key, HapChannel};
use super::bplist::{self, Plist};
use super::rtsp::{Method, RtspRequest, RtspResponse};

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

/// The receiver's ports parsed from a SETUP audio-stream response plist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ap2StreamPorts {
    /// Where audio datagrams go (`dataPort`).
    pub data_port: u16,
    /// Where sync datagrams go (`controlPort`).
    pub control_port: u16,
}

/// Pull the receiver's `dataPort`/`controlPort` out of a SETUP streams response.
pub fn parse_stream_ports(response: &Plist) -> Option<Ap2StreamPorts> {
    let stream = match response.get("streams")? {
        Plist::Array(items) => items.first()?,
        _ => return None,
    };
    Some(Ap2StreamPorts {
        data_port: stream.get("dataPort")?.as_integer()? as u16,
        control_port: stream.get("controlPort")?.as_integer()? as u16,
    })
}

/// A failure driving the AirPlay 2 control channel.
#[derive(Debug)]
pub enum Ap2Error {
    Io(std::io::Error),
    Channel(super::ap2_channel::ChannelError),
    Rejected { step: &'static str, status: u16 },
    BadBody(&'static str),
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
        }
    }
}

impl std::error::Error for Ap2Error {}

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

#[cfg(test)]
mod tests {
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
                    "content-length" => content_length = v.trim().parse().unwrap_or(0),
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
}
