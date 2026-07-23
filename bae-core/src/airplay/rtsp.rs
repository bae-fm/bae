//! A minimal RTSP client for the AirPlay control connection.
//!
//! RTSP is HTTP-shaped: a request line, `Key: Value` headers, a blank line, then
//! an optional body, with a monotonic `CSeq` header pairing each response to its
//! request. AirPlay drives both dialects over this one connection — OPTIONS,
//! ANNOUNCE, SETUP, RECORD, SET_PARAMETER, FLUSH, TEARDOWN (§7.1 of the
//! openairplay spec), plus the AirPlay 2 pairing POSTs.
//!
//! The request/response *codec* here is pure and independently tested; the
//! [`RtspConnection`] wraps it over a TCP stream and owns the `CSeq` counter and
//! the session's shared headers. The session (which issues the per-dialect
//! sequence) lives above this and drives it from its own thread, the way the Cast
//! and UPnP channels own their sockets.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

/// The RTSP methods AirPlay uses. `Method::Other` carries any additional verb a
/// dialect needs (the AirPlay 2 pairing flow POSTs, `GET_PARAMETER`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Options,
    Announce,
    Setup,
    Record,
    SetParameter,
    Flush,
    Teardown,
    Post,
    /// AirPlay 2 timing-peer list (`SETPEERS`).
    SetPeers,
    /// AirPlay 2 RTP-timeline anchor (`SETRATEANCHORTIME`).
    SetRateAnchorTime,
    Other(String),
}

impl Method {
    pub fn as_str(&self) -> &str {
        match self {
            Method::Options => "OPTIONS",
            Method::Announce => "ANNOUNCE",
            Method::Setup => "SETUP",
            Method::Record => "RECORD",
            Method::SetParameter => "SET_PARAMETER",
            Method::Flush => "FLUSH",
            Method::Teardown => "TEARDOWN",
            Method::Post => "POST",
            Method::SetPeers => "SETPEERS",
            Method::SetRateAnchorTime => "SETRATEANCHORTIME",
            Method::Other(s) => s,
        }
    }
}

/// One RTSP request: a method, a request URI, ordered headers, and a body. The
/// `CSeq`, `User-Agent`, and `Session` headers are added by [`RtspConnection`],
/// not carried here, so a request value only names what's specific to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtspRequest {
    pub method: Method,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The `Content-Type` for a request with a body, serialized as its own header.
    pub content_type: Option<String>,
}

impl RtspRequest {
    /// A request with no body (OPTIONS, RECORD, FLUSH, TEARDOWN).
    pub fn new(method: Method, uri: impl Into<String>) -> Self {
        RtspRequest {
            method,
            uri: uri.into(),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: None,
        }
    }

    /// A request carrying a typed body (ANNOUNCE's SDP, SET_PARAMETER's text, a
    /// pairing plist/TLV8).
    pub fn with_body(
        method: Method,
        uri: impl Into<String>,
        content_type: impl Into<String>,
        body: Vec<u8>,
    ) -> Self {
        RtspRequest {
            method,
            uri: uri.into(),
            headers: Vec::new(),
            body,
            content_type: Some(content_type.into()),
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    /// Serialize the request onto the wire. `cseq` and any `extra` connection
    /// headers (User-Agent, Session) are written after the request's own headers;
    /// `Content-Length` is always emitted (0 for a bodyless request), matching
    /// what receivers expect.
    pub fn serialize(&self, cseq: u32, extra: &[(String, String)]) -> Vec<u8> {
        let mut out = Vec::new();
        let _ = write!(out, "{} {} RTSP/1.0\r\n", self.method.as_str(), self.uri);
        let _ = write!(out, "CSeq: {cseq}\r\n");
        for (key, value) in extra {
            let _ = write!(out, "{key}: {value}\r\n");
        }
        for (key, value) in &self.headers {
            let _ = write!(out, "{key}: {value}\r\n");
        }
        if let Some(content_type) = &self.content_type {
            let _ = write!(out, "Content-Type: {content_type}\r\n");
        }
        let _ = write!(out, "Content-Length: {}\r\n", self.body.len());
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

/// One parsed RTSP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtspResponse {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RtspResponse {
    /// A header value looked up case-insensitively (RTSP header names are).
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn cseq(&self) -> Option<u32> {
        self.header("CSeq").and_then(|v| v.trim().parse().ok())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Read one response from `reader`: the status line, headers to the blank
    /// line, then exactly `Content-Length` body bytes (none when the header is
    /// absent).
    pub fn read_from(reader: &mut impl BufRead) -> io::Result<RtspResponse> {
        let mut status_line = String::new();
        if reader.read_line(&mut status_line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "RTSP connection closed before a response",
            ));
        }
        let (status, reason) = parse_status_line(&status_line)?;

        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "RTSP headers truncated",
                ));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.push((key.trim().to_string(), value.trim().to_string()));
            }
        }

        let content_length = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;

        Ok(RtspResponse {
            status,
            reason,
            headers,
            body,
        })
    }
}

fn parse_status_line(line: &str) -> io::Result<(u16, String)> {
    // `RTSP/1.0 200 OK`
    let mut parts = line.trim_end().splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("RTSP/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not an RTSP status line: {line:?}"),
        ));
    }
    let status = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing RTSP status code"))?;
    let reason = parts.next().unwrap_or("").trim().to_string();
    Ok((status, reason))
}

/// The live RTSP control connection to a receiver: a TCP stream, the monotonic
/// `CSeq`, and the headers every request on this connection carries. Owned and
/// driven on one thread (each call blocks on the socket), matching how the Cast
/// and UPnP channels own theirs.
pub struct RtspConnection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    cseq: u32,
    /// Sent as `User-Agent` on every request. Receivers key some behavior off it.
    user_agent: String,
    /// Set from a SETUP response's `Session` header; echoed on later requests.
    session: Option<String>,
    /// The local address the receiver sees us on, for building request URIs and
    /// SDP connection lines.
    local_addr: IpAddr,
}

impl RtspConnection {
    /// Open the RTSP control connection to `addr:port`.
    pub fn connect(addr: IpAddr, port: u16, user_agent: impl Into<String>) -> io::Result<Self> {
        let stream = TcpStream::connect(SocketAddr::new(addr, port))?;
        stream.set_nodelay(true).ok();
        Self::from_stream(stream, user_agent)
    }

    /// Build a connection over an already-open stream (loopback fakes in tests use
    /// this).
    pub fn from_stream(stream: TcpStream, user_agent: impl Into<String>) -> io::Result<Self> {
        let local_addr = stream.local_addr()?.ip();
        let reader = BufReader::new(stream.try_clone()?);
        Ok(RtspConnection {
            writer: stream,
            reader,
            cseq: 0,
            user_agent: user_agent.into(),
            session: None,
            local_addr,
        })
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.writer.set_read_timeout(timeout)
    }

    /// Take the underlying TCP stream back, for handing a control connection off to
    /// the encrypted transport after AirPlay 2 pair-verify. The buffered reader is
    /// dropped — safe only when the last response was fully consumed (each RTSP
    /// request reads exactly its `Content-Length` body, so nothing is left over).
    pub fn into_stream(self) -> TcpStream {
        self.writer
    }

    pub fn local_addr(&self) -> IpAddr {
        self.local_addr
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// Send `request` and read its response, advancing `CSeq` and attaching the
    /// connection's `User-Agent` and `Session` headers. A response whose `CSeq`
    /// doesn't echo the request's is a protocol error.
    pub fn request(&mut self, request: &RtspRequest) -> io::Result<RtspResponse> {
        self.cseq += 1;
        let cseq = self.cseq;

        let mut extra = vec![("User-Agent".to_string(), self.user_agent.clone())];
        if let Some(session) = &self.session {
            extra.push(("Session".to_string(), session.clone()));
        }

        let bytes = request.serialize(cseq, &extra);
        self.writer.write_all(&bytes)?;
        self.writer.flush()?;

        let response = RtspResponse::read_from(&mut self.reader)?;
        if let Some(echoed) = response.cseq() {
            if echoed != cseq {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("RTSP CSeq mismatch: sent {cseq}, got {echoed}"),
                ));
            }
        }
        // A SETUP response establishes the session id used for the rest of the
        // connection.
        if request.method == Method::Setup {
            if let Some(session) = response.header("Session") {
                // The session id may be followed by `;timeout=...`; keep only the id.
                let id = session.split(';').next().unwrap_or(session).trim();
                self.session = Some(id.to_string());
            }
        }
        Ok(response)
    }
}

/// A request parsed by the scripted-fake test receiver.
#[cfg(test)]
struct ParsedRequest {
    method: Method,
    headers: Vec<(String, String)>,
}

#[cfg(test)]
impl ParsedRequest {
    fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// Read a full RTSP request from a stream — the receiver side, used only by the
/// scripted-fake test receiver. Mirrors [`RtspResponse::read_from`]: request
/// line, headers, then `Content-Length` body bytes (drained, not returned).
#[cfg(test)]
fn read_request(reader: &mut impl BufRead) -> io::Result<ParsedRequest> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no request"));
    }
    let method = match request_line.trim_end().split(' ').next().unwrap_or("") {
        "OPTIONS" => Method::Options,
        "ANNOUNCE" => Method::Announce,
        "SETUP" => Method::Setup,
        "RECORD" => Method::Record,
        "SET_PARAMETER" => Method::SetParameter,
        "FLUSH" => Method::Flush,
        "TEARDOWN" => Method::Teardown,
        "POST" => Method::Post,
        other => Method::Other(other.to_string()),
    };

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    Ok(ParsedRequest { method, headers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;

    #[test]
    fn serialize_bodyless_request_has_cseq_and_zero_length() {
        let req = RtspRequest::new(Method::Options, "*");
        let bytes = req.serialize(1, &[("User-Agent".into(), "bae/1".into())]);
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(
            text,
            "OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nUser-Agent: bae/1\r\nContent-Length: 0\r\n\r\n"
        );
    }

    #[test]
    fn serialize_request_with_body_emits_content_type_and_length() {
        let sdp = b"v=0\r\n".to_vec();
        let req = RtspRequest::with_body(
            Method::Announce,
            "rtsp://10.0.0.1/1",
            "application/sdp",
            sdp.clone(),
        );
        let bytes = req.serialize(2, &[]);
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("ANNOUNCE rtsp://10.0.0.1/1 RTSP/1.0\r\nCSeq: 2\r\n"));
        assert!(text.contains("Content-Type: application/sdp\r\n"));
        assert!(text.contains(&format!("Content-Length: {}\r\n", sdp.len())));
        assert!(text.ends_with("\r\n\r\nv=0\r\n"));
    }

    #[test]
    fn parse_response_reads_status_headers_and_body() {
        let raw = "RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
        let mut cursor = Cursor::new(raw.as_bytes());
        let resp = RtspResponse::read_from(&mut cursor).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.cseq(), Some(3));
        assert_eq!(resp.header("Content-Type"), Some("text/plain"));
        assert_eq!(resp.body, b"hello");
        assert!(resp.is_success());
    }

    #[test]
    fn parse_response_without_body() {
        let raw = "RTSP/1.0 200 OK\r\nCSeq: 1\r\nSession: DEADBEEF\r\n\r\n";
        let mut cursor = Cursor::new(raw.as_bytes());
        let resp = RtspResponse::read_from(&mut cursor).unwrap();
        assert_eq!(resp.body, b"");
        assert_eq!(resp.header("Session"), Some("DEADBEEF"));
    }

    #[test]
    fn non_rtsp_status_line_is_an_error() {
        let raw = "HTTP/1.1 200 OK\r\n\r\n";
        let mut cursor = Cursor::new(raw.as_bytes());
        assert!(RtspResponse::read_from(&mut cursor).is_err());
    }

    /// A loopback fake receiver: it reads each request, records the method, and
    /// replies with a canned response — no real network, no real receiver. The
    /// client drives OPTIONS → ANNOUNCE → SETUP → RECORD in order; the fake
    /// asserts that order and that SETUP's `Session` is picked up and echoed on
    /// RECORD.
    #[test]
    fn scripted_fake_receiver_drives_the_setup_sequence_in_order() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut methods = Vec::new();
            let mut record_session = None;

            for _ in 0..4 {
                let request = read_request(&mut reader).unwrap();
                let cseq = request.header("CSeq").unwrap().to_string();
                if request.method == Method::Record {
                    record_session = request.header("Session").map(str::to_string);
                }
                let extra = if request.method == Method::Setup {
                    "Session: 7A3B120C;timeout=60\r\nTransport: RTP/AVP/UDP;server_port=6000\r\n"
                } else {
                    ""
                };
                let response = format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n{extra}\r\n");
                writer.write_all(response.as_bytes()).unwrap();
                writer.flush().unwrap();
                methods.push(request.method);
            }
            (methods, record_session)
        });

        let mut conn = RtspConnection::connect(addr.ip(), addr.port(), "bae/test").unwrap();
        assert_eq!(
            conn.request(&RtspRequest::new(Method::Options, "*"))
                .unwrap()
                .status,
            200
        );
        conn.request(&RtspRequest::with_body(
            Method::Announce,
            "rtsp://x/1",
            "application/sdp",
            b"v=0\r\n".to_vec(),
        ))
        .unwrap();
        conn.request(
            &RtspRequest::new(Method::Setup, "rtsp://x/1")
                .header("Transport", "RTP/AVP/UDP;unicast"),
        )
        .unwrap();
        // The session id from SETUP is now attached automatically.
        assert_eq!(conn.session(), Some("7A3B120C"));
        conn.request(&RtspRequest::new(Method::Record, "rtsp://x/1"))
            .unwrap();

        let (methods, record_session) = server.join().unwrap();
        assert_eq!(
            methods,
            vec![
                Method::Options,
                Method::Announce,
                Method::Setup,
                Method::Record
            ]
        );
        assert_eq!(
            record_session.as_deref(),
            Some("7A3B120C"),
            "the session id from SETUP is echoed on RECORD"
        );
    }
}
