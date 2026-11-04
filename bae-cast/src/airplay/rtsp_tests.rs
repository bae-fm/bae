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
    let raw =
        "RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
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
            let request = read_request(&mut reader).unwrap().expect("the sender sent a request");
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
    assert_eq!(conn.request(&RtspRequest::new(Method::Options, "*")).unwrap().status, 200);
    conn.request(&RtspRequest::with_body(
        Method::Announce,
        "rtsp://x/1",
        "application/sdp",
        b"v=0\r\n".to_vec(),
    ))
    .unwrap();
    conn.request(
        &RtspRequest::new(Method::Setup, "rtsp://x/1").header("Transport", "RTP/AVP/UDP;unicast"),
    )
    .unwrap();
    // The session id from SETUP is now attached automatically.
    assert_eq!(conn.session(), Some("7A3B120C"));
    conn.request(&RtspRequest::new(Method::Record, "rtsp://x/1")).unwrap();

    let (methods, record_session) = server.join().unwrap();
    assert_eq!(methods, vec![Method::Options, Method::Announce, Method::Setup, Method::Record]);
    assert_eq!(
        record_session.as_deref(),
        Some("7A3B120C"),
        "the session id from SETUP is echoed on RECORD"
    );
}
