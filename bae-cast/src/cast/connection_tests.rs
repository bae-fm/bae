use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::task::{Context, Poll};

use bae_util::test_channels::HANG_GUARD;
use serde_json::json;
use tokio::io::{DuplexStream, ReadBuf};

use super::super::fake_receiver::FakeReceiver;
use super::super::messages::{
    NAMESPACE_CONNECTION, NAMESPACE_HEARTBEAT, NAMESPACE_MEDIA, NAMESPACE_RECEIVER,
};
use super::*;

/// A stream whose writes fail once `fail_writes` is set.
struct FlakyWrites {
    inner: DuplexStream,
    fail_writes: Arc<AtomicBool>,
}

impl AsyncRead for FlakyWrites {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for FlakyWrites {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.fail_writes.load(Ordering::SeqCst) {
            return Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)));
        }
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

struct Harness {
    connection: CastConnection,
    fake: FakeReceiver,
    messages: mpsc::UnboundedReceiver<Inbound>,
    ends: Arc<Mutex<Vec<End>>>,
}

impl Harness {
    fn ends(&self) -> Vec<End> {
        self.ends.lock().unwrap().clone()
    }
}

fn handler_into(messages: mpsc::UnboundedSender<Inbound>) -> Handler {
    Box::new(move |inbound| {
        messages.send(inbound).expect("the test holds the messages");
        Ok(())
    })
}

fn recording_end(ends: &Arc<Mutex<Vec<End>>>) -> OnEnd {
    let ends = ends.clone();
    Box::new(move |end| ends.lock().unwrap().push(end.clone()))
}

fn harness_with(handler: Option<Handler>, fail_writes: Option<Arc<AtomicBool>>) -> Harness {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve(fake_end);
    let (to_test, messages) = mpsc::unbounded_channel();
    let ends = Arc::new(Mutex::new(Vec::new()));
    let handler = handler.unwrap_or_else(|| handler_into(to_test));
    let diagnostics = Diagnostics::noop();
    let runtime = tokio::runtime::Handle::current();
    let connection = match fail_writes {
        Some(fail_writes) => CastConnection::start(
            FlakyWrites { inner: client_end, fail_writes },
            &diagnostics,
            &runtime,
            handler,
            recording_end(&ends),
        ),
        None => {
            CastConnection::start(client_end, &diagnostics, &runtime, handler, recording_end(&ends))
        }
    };
    Harness { connection, fake, messages, ends }
}

fn harness() -> Harness {
    harness_with(None, None)
}

fn status_request(connection: &CastConnection, request_id: u32) {
    connection
        .send(Request::SetVolume { request_id, level: 0.5 }, "sender-0", "receiver-0")
        .expect("the connection is up");
}

fn receiver_status(request_id: u32) -> serde_json::Value {
    json!({"type": "RECEIVER_STATUS", "requestId": request_id, "status": {"volume": {"level": 1.0}}})
}

/// Let every task that can run, run.
async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn a_ping_arriving_while_a_request_waits_gets_its_pong_at_once() {
    let mut h = harness();
    status_request(&h.connection, 1);
    let request = h.fake.next_received().await;
    assert_eq!((request.namespace.as_str(), request.kind()), (NAMESPACE_RECEIVER, "SET_VOLUME"));

    h.fake.ping();
    let pong = h.fake.next_received().await;
    assert_eq!((pong.namespace.as_str(), pong.kind()), (NAMESPACE_HEARTBEAT, "PONG"));
    assert_eq!((pong.source_id.as_str(), pong.destination_id.as_str()), ("sender-0", "receiver-0"));

    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(1));
    let answer = tokio::time::timeout(HANG_GUARD, h.messages.recv()).await.unwrap().unwrap();
    assert!(matches!(answer, Inbound::ReceiverStatus { request_id: 1, .. }));
}

#[tokio::test]
async fn messages_reach_the_handler_in_order_and_pings_and_pongs_do_not() {
    let mut h = harness();
    h.fake.ping();
    h.fake.send_json(NAMESPACE_HEARTBEAT, json!({"type": "PONG"}));
    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(1));
    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(2));
    for expected in [1, 2] {
        let message = tokio::time::timeout(HANG_GUARD, h.messages.recv()).await.unwrap().unwrap();
        assert!(
            matches!(message, Inbound::ReceiverStatus { request_id, .. } if request_id == expected)
        );
    }
}

#[tokio::test]
async fn a_frame_on_a_namespace_bae_does_not_use_is_dropped_and_the_connection_goes_on() {
    let mut h = harness();
    h.fake.send_json("urn:x-cast:com.example.other", json!({"type": "X"}));
    h.fake.send_json(NAMESPACE_MEDIA, json!({"responseType": "GET_STATUS"}));
    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(7));
    let message = tokio::time::timeout(HANG_GUARD, h.messages.recv()).await.unwrap().unwrap();
    assert!(matches!(message, Inbound::ReceiverStatus { request_id: 7, .. }));
    assert!(h.connection.end().is_none());
}

#[tokio::test(start_paused = true)]
async fn after_five_seconds_without_a_frame_one_ping_is_sent() {
    let mut h = harness();
    settle().await;
    tokio::time::advance(Duration::from_millis(4900)).await;
    settle().await;
    assert!(h.fake.try_received().is_none(), "no ping before five seconds");

    tokio::time::advance(Duration::from_millis(200)).await;
    settle().await;
    let ping = h.fake.try_received().expect("a ping after five seconds");
    assert_eq!((ping.namespace.as_str(), ping.kind()), (NAMESPACE_HEARTBEAT, "PING"));
    assert!(h.fake.try_received().is_none(), "one ping");
}

#[tokio::test(start_paused = true)]
async fn every_frame_received_restarts_the_keep_alive() {
    let mut h = harness();
    settle().await;
    tokio::time::advance(Duration::from_secs(4)).await;
    h.fake.send_json(NAMESPACE_HEARTBEAT, json!({"type": "PONG"}));
    settle().await;
    tokio::time::advance(Duration::from_secs(4)).await;
    settle().await;
    assert!(h.fake.try_received().is_none(), "eight seconds in, four since the last frame");

    tokio::time::advance(Duration::from_millis(1100)).await;
    settle().await;
    assert_eq!(h.fake.try_received().expect("a ping").kind(), "PING");
}

#[tokio::test(start_paused = true)]
async fn a_receiver_silent_ten_seconds_after_the_ping_is_lost_once() {
    let mut h = harness();
    h.fake.go_silent();
    settle().await;
    tokio::time::advance(Duration::from_millis(5100)).await;
    settle().await;
    assert_eq!(h.fake.try_received().expect("a ping").kind(), "PING");

    // The liveness clock starts at the ping: lost at 15 s, not 10 s.
    tokio::time::advance(Duration::from_millis(9800)).await;
    settle().await;
    assert!(h.connection.end().is_none(), "14.9 s in, 9.8 s after the ping");

    tokio::time::advance(Duration::from_millis(300)).await;
    let end = h.connection.ended().await;
    let End::Lost(reason) = end else { panic!("not a loss: {end:?}") };
    assert!(reason.contains("nothing for 10 s after a ping"), "{reason}");
    h.connection.tasks_done().await;
    assert_eq!(h.ends().len(), 1, "reported once");
}

#[tokio::test(start_paused = true)]
async fn a_frame_after_the_ping_keeps_the_connection() {
    let mut h = harness();
    settle().await;
    tokio::time::advance(Duration::from_millis(5100)).await;
    settle().await;
    assert_eq!(h.fake.try_received().expect("a ping").kind(), "PING");
    tokio::time::advance(Duration::from_secs(7)).await;
    h.fake.send_json(NAMESPACE_HEARTBEAT, json!({"type": "PONG"}));
    settle().await;
    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert!(h.connection.end().is_none());
}

#[tokio::test(start_paused = true)]
async fn a_ping_that_cannot_be_written_ends_the_connection_as_lost() {
    let fail_writes = Arc::new(AtomicBool::new(false));
    let h = harness_with(None, Some(fail_writes.clone()));
    settle().await;
    fail_writes.store(true, Ordering::SeqCst);
    tokio::time::advance(Duration::from_millis(5100)).await;
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("writing failed"), "{reason}");
    assert_eq!(h.ends().len(), 1);
}

#[tokio::test]
async fn a_pong_that_cannot_be_written_ends_the_connection_as_lost() {
    let fail_writes = Arc::new(AtomicBool::new(true));
    let h = harness_with(None, Some(fail_writes));
    h.fake.ping();
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("writing failed"), "{reason}");
    assert_eq!(h.ends().len(), 1);
}

#[tokio::test]
async fn the_receiver_closing_the_stream_ends_the_connection_as_lost_by_the_receiver() {
    let h = harness();
    h.fake.drop_connection();
    assert_eq!(h.connection.ended().await, End::Lost("closed by the receiver".to_string()));
    h.connection.tasks_done().await;
    assert_eq!(h.ends(), vec![End::Lost("closed by the receiver".to_string())]);
}

#[tokio::test]
async fn a_request_sent_after_the_end_fails_with_the_end_s_reason() {
    let h = harness();
    h.fake.drop_connection();
    h.connection.ended().await;
    let error = h
        .connection
        .send(Request::Ping, "sender-0", "receiver-0")
        .expect_err("the connection is over");
    assert_eq!(error, RendererError::Connection("closed by the receiver".to_string()));
}

#[tokio::test]
async fn a_refused_frame_ends_the_connection_and_the_frames_after_it_are_not_taken() {
    let mut h = harness();
    let mut bytes = ((super::super::wire::MAX_BODY_LEN + 1) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(&encode_frame(&Request::Ping.into_frame("a", "b")).unwrap());
    h.fake.send_raw(bytes);
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("refuses"), "{reason}");
    h.connection.tasks_done().await;
    assert!(h.messages.try_recv().is_err());
    assert_eq!(h.ends().len(), 1);
}

#[tokio::test]
async fn a_chunked_frame_ends_the_connection() {
    let h = harness();
    let mut chunk = Request::Ping.into_frame("receiver-0", "sender-0");
    chunk.continued = Some(true);
    h.fake.send_raw(encode_frame(&chunk).unwrap());
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("chunk"), "{reason}");
}

#[tokio::test]
async fn a_known_message_that_cannot_be_read_ends_the_connection() {
    let h = harness();
    h.fake.send_json(
        NAMESPACE_MEDIA,
        json!({"type": "MEDIA_STATUS", "status": [{"mediaSessionId": 1}]}),
    );
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("MEDIA_STATUS"), "{reason}");
}

#[tokio::test]
async fn a_handler_that_refuses_a_message_ends_the_connection_with_its_reason() {
    let h = harness_with(Some(Box::new(|_| Err("another sender took over".to_string()))), None);
    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(1));
    assert_eq!(h.connection.ended().await, End::Lost("another sender took over".to_string()));
    assert_eq!(h.ends().len(), 1);
}

#[tokio::test]
async fn a_panic_in_the_handler_ends_the_connection_with_the_panic_as_its_cause() {
    let h = harness_with(Some(Box::new(|_| panic!("handler broke"))), None);
    h.fake.send_json(NAMESPACE_RECEIVER, receiver_status(1));
    let End::Lost(reason) = h.connection.ended().await else { panic!("not a loss") };
    assert!(reason.contains("handler broke"), "{reason}");
    h.connection.tasks_done().await;
    assert_eq!(h.ends().len(), 1);
}

#[tokio::test]
async fn only_the_first_end_counts() {
    let h = harness();
    h.fake.drop_connection();
    h.connection.ended().await;
    h.connection.close(vec![]).await;
    h.shared_set_end_again();
    assert_eq!(h.ends(), vec![End::Lost("closed by the receiver".to_string())]);
    assert_eq!(h.connection.end(), Some(&End::Lost("closed by the receiver".to_string())));
}

impl Harness {
    fn shared_set_end_again(&self) {
        self.connection.shared.lose("a second reason".to_string());
    }
}

#[tokio::test]
async fn closing_sends_the_farewells_in_order_then_ends_the_stream_and_is_not_a_loss() {
    let mut h = harness();
    let to_transport = Request::Close.into_frame("sender-0", "transport-1");
    let to_platform = Request::Close.into_frame("sender-0", "receiver-0");
    h.connection.close(vec![to_transport, to_platform]).await;

    let first = h.fake.next_received().await;
    let second = h.fake.next_received().await;
    assert_eq!(
        (first.namespace.as_str(), first.kind(), first.destination_id.as_str()),
        (NAMESPACE_CONNECTION, "CLOSE", "transport-1")
    );
    assert_eq!((second.kind(), second.destination_id.as_str()), ("CLOSE", "receiver-0"));
    assert!(h.fake.next_received_or_end().await.is_none(), "then the stream closed");
    assert_eq!(h.ends(), vec![End::Closed]);
    h.connection.tasks_done().await;
    assert!(h.connection.send(Request::Ping, "a", "b").is_err());
}

#[tokio::test]
async fn dropping_the_connection_closes_the_socket_without_waiting() {
    let Harness { connection, mut fake, ends, .. } = harness();
    drop(connection);
    assert!(fake.next_received_or_end().await.is_none(), "the receiver sees the stream end");
    assert!(ends.lock().unwrap().is_empty(), "a drop reports nothing");
}

#[tokio::test]
async fn bae_sets_no_tls_key_log() {
    std::env::set_var("SSLKEYLOGFILE", "/nonexistent/keys.log");
    let config = tls_config();
    std::env::remove_var("SSLKEYLOGFILE");
    assert!(!config.key_log.will_log("CLIENT_RANDOM"));
}

async fn open_to(
    port: u16,
    ends: &Arc<Mutex<Vec<End>>>,
) -> (Result<CastConnection, RendererError>, mpsc::UnboundedReceiver<Inbound>) {
    let (to_test, messages) = mpsc::unbounded_channel();
    let opened = CastConnection::open(
        IpAddr::from([127, 0, 0, 1]),
        port,
        &Diagnostics::noop(),
        &tokio::runtime::Handle::current(),
        handler_into(to_test),
        recording_end(ends),
    )
    .await;
    (opened, messages)
}

#[tokio::test]
async fn a_connection_over_tls_to_a_self_signed_receiver_carries_messages_both_ways() {
    let (mut fake, port) = FakeReceiver::serve_tls(false).await;
    let ends = Arc::new(Mutex::new(Vec::new()));
    let (opened, mut messages) = open_to(port, &ends).await;
    let connection = opened.expect("the receiver's self-signed certificate is accepted");

    status_request(&connection, 3);
    assert_eq!(fake.next_received().await.kind(), "SET_VOLUME");
    fake.send_json(NAMESPACE_RECEIVER, receiver_status(3));
    let answer = tokio::time::timeout(HANG_GUARD, messages.recv()).await.unwrap().unwrap();
    assert!(matches!(answer, Inbound::ReceiverStatus { request_id: 3, .. }));
}

#[tokio::test]
async fn a_refused_connect_fails_as_a_connection_error() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let ends = Arc::new(Mutex::new(Vec::new()));
    let (opened, _messages) = open_to(port, &ends).await;
    assert!(matches!(opened, Err(RendererError::Connection(_))));
    assert!(ends.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_tls_handshake_fails_as_a_connection_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = async {
        let (mut tcp, _) = listener.accept().await.unwrap();
        tcp.write_all(b"this is not a TLS server hello\r\n").await.unwrap();
        tcp
    };
    let ends = Arc::new(Mutex::new(Vec::new()));
    let (served, (opened, _messages)) = tokio::join!(server, open_to(port, &ends));
    drop(served);
    assert!(matches!(opened, Err(RendererError::Connection(_))));
    assert!(ends.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_receiver_that_never_answers_the_handshake_fails_the_connect_after_ten_seconds() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ends = Arc::new(Mutex::new(Vec::new()));
    let (opened, _messages) = open_to(port, &ends).await;
    let Err(RendererError::Connection(reason)) = opened else { panic!("did not fail") };
    assert!(reason.contains("within 10 s"), "{reason}");
    drop(listener);
}
