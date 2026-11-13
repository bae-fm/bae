//! A fake Cast receiver for the client's tests.
//!
//! It serves one client over any byte stream, reading and writing frames
//! through the same [`wire`](super::wire) and [`messages`](super::messages)
//! the client uses. It answers a PING with a PONG, records every message it
//! receives in order, and does what its test handles tell it to.

use std::sync::Arc;

use bae_diagnostics::{Diagnostics, ReportedTask};
use bae_util::test_channels::HANG_GUARD;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::messages::{Request, NAMESPACE_HEARTBEAT};
use super::wire::{encode_frame, CastMessage, FrameReader};

/// A message the fake received from the client.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Received {
    pub(crate) source_id: String,
    pub(crate) destination_id: String,
    pub(crate) namespace: String,
    pub(crate) payload: Value,
}

impl Received {
    /// The payload's `type`.
    pub(crate) fn kind(&self) -> &str {
        self.payload.get("type").and_then(Value::as_str).unwrap_or("")
    }
}

enum Control {
    Send(CastMessage),
    SendRaw(Vec<u8>),
    GoSilent,
    DropConnection,
}

pub(crate) struct FakeReceiver {
    control: mpsc::UnboundedSender<Control>,
    received: mpsc::UnboundedReceiver<Received>,
    _task: ReportedTask,
}

impl FakeReceiver {
    /// Serve the client at the other end of `stream`. Must run inside a
    /// tokio runtime.
    pub(crate) fn serve<S>(stream: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (control, commands) = mpsc::unbounded_channel();
        let (record, received) = mpsc::unbounded_channel();
        let task = Diagnostics::noop().spawn_reported(
            &tokio::runtime::Handle::current(),
            "fake cast receiver",
            serve(stream, commands, record),
            |bug| panic!("the fake receiver panicked: {bug}"),
        );
        Self { control, received, _task: task }
    }

    /// Listen on `127.0.0.1:0`, take one connection and serve it over TLS with
    /// a certificate made for the test. Returns the fake and its port.
    pub(crate) async fn serve_tls() -> (Self, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind a local port");
        let port = listener.local_addr().expect("a bound address").port();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
                .expect("a self-signed certificate");
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("the provider supports the safe default protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key)
        .expect("a certificate and its key");
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

        let (control, commands) = mpsc::unbounded_channel();
        let (record, received) = mpsc::unbounded_channel();
        let task = Diagnostics::noop().spawn_reported(
            &tokio::runtime::Handle::current(),
            "fake cast receiver",
            async move {
                let (tcp, _) = listener.accept().await.expect("the client connects");
                let tls = acceptor.accept(tcp).await.expect("the TLS handshake");
                serve(tls, commands, record).await;
            },
            |bug| panic!("the fake receiver panicked: {bug}"),
        );
        (Self { control, received, _task: task }, port)
    }

    /// Send the client a message from `receiver-0` to `sender-0`.
    pub(crate) fn send_json(&self, namespace: &str, payload: Value) {
        self.send_from("receiver-0", namespace, payload);
    }

    /// Send the client a message from `source_id` to `sender-0`.
    pub(crate) fn send_from(&self, source_id: &str, namespace: &str, payload: Value) {
        let frame = CastMessage::text(source_id, "sender-0", namespace, payload.to_string());
        self.control.send(Control::Send(frame)).expect("the fake receiver is serving");
    }

    /// PING the client.
    pub(crate) fn ping(&self) {
        self.send_json(NAMESPACE_HEARTBEAT, serde_json::json!({"type": "PING"}));
    }

    /// Send these bytes as they are.
    pub(crate) fn send_raw(&self, bytes: Vec<u8>) {
        self.control.send(Control::SendRaw(bytes)).expect("the fake receiver is serving");
    }

    /// Keep reading, but never answer and never ping.
    pub(crate) fn go_silent(&self) {
        self.control.send(Control::GoSilent).expect("the fake receiver is serving");
    }

    /// Close the connection.
    pub(crate) fn drop_connection(&self) {
        self.control.send(Control::DropConnection).expect("the fake receiver is serving");
    }

    /// A message the client already sent and nobody has taken yet.
    pub(crate) fn try_received(&mut self) -> Option<Received> {
        self.received.try_recv().ok()
    }

    /// The next message the client sent, or `None` once the fake receiver has
    /// stopped serving (the client closed its end, or it was told to drop).
    pub(crate) async fn next_received_or_end(&mut self) -> Option<Received> {
        tokio::time::timeout(HANG_GUARD, self.received.recv())
            .await
            .expect("the client sent a message or ended in time")
    }

    /// The next message the client sent, in the order it sent them.
    pub(crate) async fn next_received(&mut self) -> Received {
        tokio::time::timeout(HANG_GUARD, self.received.recv())
            .await
            .expect("the client sent a message in time")
            .expect("the fake receiver is serving")
    }
}

async fn serve<S>(
    stream: S,
    mut commands: mpsc::UnboundedReceiver<Control>,
    record: mpsc::UnboundedSender<Received>,
) where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut frames = FrameReader::new(read_half);
    let mut silent = false;
    loop {
        tokio::select! {
            frame = frames.next() => {
                let Ok(Some(frame)) = frame else { return };
                let payload: Value = serde_json::from_str(frame.payload_utf8.as_deref().unwrap_or("{}"))
                    .expect("the client sends JSON");
                let received = Received {
                    source_id: frame.source_id,
                    destination_id: frame.destination_id,
                    namespace: frame.namespace,
                    payload,
                };
                let is_ping = received.namespace == NAMESPACE_HEARTBEAT && received.kind() == "PING";
                let _recorded = record.send(received);
                if is_ping && !silent {
                    let pong = Request::Pong.into_frame("receiver-0", "sender-0");
                    if write_half.write_all(&encode_frame(&pong).expect("a PONG fits")).await.is_err() {
                        return;
                    }
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { return };
                match command {
                    Control::Send(frame) => {
                        let bytes = encode_frame(&frame).expect("a test frame fits");
                        if write_half.write_all(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Control::SendRaw(bytes) => {
                        if write_half.write_all(&bytes).await.is_err() {
                            return;
                        }
                    }
                    Control::GoSilent => silent = true,
                    Control::DropConnection => return,
                }
            }
        }
    }
}
