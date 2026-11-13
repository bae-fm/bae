//! One Cast connection: a TLS stream split into a reader and a writer task.
//!
//! The reader reads every frame as it arrives, answers a PING at once through
//! the writer's queue, keeps the connection alive, and hands every other
//! message to the owner's handler. It never waits on the writer, and the
//! writer never waits on the reader, so a request waiting for its answer never
//! holds up a PONG.
//!
//! A connection ends once, for one reason: [`End`]. The first end set wins.
//! Both tasks run on the runtime handed to [`CastConnection::start`], never on
//! the caller's.

use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bae_diagnostics::{Diagnostics, ReportedTask};
use bae_playback::renderer::RendererError;
use bae_util::error_text::CauseChain;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerifier};
use rustls::crypto::{aws_lc_rs, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::messages::{Inbound, Request};
use super::wire::{encode_frame, CastMessage, FrameReader, WireError};

/// How long to connect, TLS handshake included.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A receiver that has sent nothing for this long is sent a PING.
pub(crate) const PING_AFTER: Duration = Duration::from_secs(5);
/// A receiver that sends nothing for this long after a PING is lost.
pub(crate) const LOST_AFTER_PING: Duration = Duration::from_secs(10);
/// How long one frame may take to write, and the close to finish.
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum End {
    /// The link failed or the receiver went: the reason in words.
    Lost(String),
    /// bae closed it.
    Closed,
}

/// What the owner does with each message the reader takes off the stream. An
/// `Err` is the reason the connection is lost.
pub(crate) type Handler = Box<dyn FnMut(Inbound) -> Result<(), String> + Send>;

/// Called once, by whichever side set the end, as it is set.
pub(crate) type OnEnd = Box<dyn Fn(&End) + Send + Sync>;

enum Job {
    Frame(Vec<u8>),
    /// Everything before it is written; then the stream is shut down.
    Shutdown,
}

struct Shared {
    end: OnceLock<End>,
    on_end: OnEnd,
    /// Cancelled when the end is set: the reader stops.
    stop_reader: CancellationToken,
    /// Cancelled when the end is a loss: the writer stops at once. A close
    /// lets it write what is queued first.
    stop_writer: CancellationToken,
    ended: CancellationToken,
    /// Cancelled as each task ends, however it ends.
    reader_done: CancellationToken,
    writer_done: CancellationToken,
}

impl Shared {
    fn set_end(&self, end: End) {
        if self.end.set(end).is_err() {
            return;
        }
        let end = self.end.get().expect("the end was just set");
        self.stop_reader.cancel();
        if matches!(end, End::Lost(_)) {
            self.stop_writer.cancel();
        }
        (self.on_end)(end);
        self.ended.cancel();
    }

    fn lose(&self, reason: String) {
        self.set_end(End::Lost(reason));
    }
}

/// A live Cast connection. Dropping it aborts both tasks, which closes the
/// socket, and never blocks.
pub(crate) struct CastConnection {
    shared: Arc<Shared>,
    jobs: mpsc::UnboundedSender<Job>,
    tasks: [ReportedTask; 2],
}

impl CastConnection {
    /// Connect to the receiver at `addr:port` over TCP and TLS within
    /// [`CONNECT_TIMEOUT`], then [`start`](Self::start) on `runtime`. A connect
    /// that fails leaves no task and no socket behind.
    pub(crate) async fn open(
        addr: IpAddr,
        port: u16,
        diagnostics: &Diagnostics,
        runtime: &tokio::runtime::Handle,
        handler: Handler,
        on_end: OnEnd,
    ) -> Result<Self, RendererError> {
        let connect = async {
            let tcp = TcpStream::connect((addr, port)).await?;
            let connector = TlsConnector::from(Arc::new(tls_config()));
            connector.connect(ServerName::IpAddress(addr.into()), tcp).await
        };
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, connect)
            .await
            .map_err(|_elapsed| {
                RendererError::Connection(format!(
                    "no connection to {addr}:{port} within {} s",
                    CONNECT_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|e| RendererError::Connection(CauseChain(&e).to_string()))?;
        Ok(Self::start(stream, diagnostics, runtime, handler, on_end))
    }

    /// Run a connection over `stream`: a reader task and a writer task.
    pub(crate) fn start<S>(
        stream: S,
        diagnostics: &Diagnostics,
        runtime: &tokio::runtime::Handle,
        handler: Handler,
        on_end: OnEnd,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let shared = Arc::new(Shared {
            end: OnceLock::new(),
            on_end,
            stop_reader: CancellationToken::new(),
            stop_writer: CancellationToken::new(),
            ended: CancellationToken::new(),
            reader_done: CancellationToken::new(),
            writer_done: CancellationToken::new(),
        });
        let (read_half, write_half) = tokio::io::split(stream);
        let (jobs, queue) = mpsc::unbounded_channel();

        let reader = diagnostics.spawn_reported(
            runtime,
            "cast connection reader",
            read_loop(read_half, handler, jobs.clone(), shared.clone()),
            panic_ends(&shared),
        );
        let writer = diagnostics.spawn_reported(
            runtime,
            "cast connection writer",
            write_loop(write_half, queue, shared.clone()),
            panic_ends(&shared),
        );
        Self { shared, jobs, tasks: [reader, writer] }
    }

    /// Queue `request` for the writer. Fails with the end's reason once the
    /// connection has ended.
    pub(crate) fn send(
        &self,
        request: Request<'_>,
        source_id: &str,
        destination_id: &str,
    ) -> Result<(), RendererError> {
        self.send_frame(request.into_frame(source_id, destination_id))
    }

    pub(crate) fn send_frame(&self, frame: CastMessage) -> Result<(), RendererError> {
        if let Some(end) = self.shared.end.get() {
            return Err(ended_error(end));
        }
        let bytes =
            encode_frame(&frame).map_err(|e| RendererError::Command(CauseChain(&e).to_string()))?;
        self.jobs.send(Job::Frame(bytes)).map_err(|_unsent| {
            ended_error(self.shared.end.get().unwrap_or(&End::Lost("the writer is gone".into())))
        })
    }

    /// Close in order: `farewells` are queued, then a shutdown, and the end
    /// is [`End::Closed`]. The writer sends them and the TLS close within
    /// [`WRITE_TIMEOUT`] each, then both tasks end, and this returns. Does
    /// nothing if the connection already ended.
    pub(crate) async fn close(&self, farewells: Vec<CastMessage>) {
        if self.shared.end.get().is_some() {
            return;
        }
        for frame in farewells {
            match encode_frame(&frame) {
                Ok(bytes) => {
                    let _queued = self.jobs.send(Job::Frame(bytes));
                }
                Err(error) => warn!("a closing frame was not sent: {error}"),
            }
        }
        let _queued = self.jobs.send(Job::Shutdown);
        self.shared.set_end(End::Closed);
        self.shared.writer_done.cancelled().await;
    }

    /// Wait until both tasks have ended.
    pub(crate) async fn tasks_done(&self) {
        self.shared.reader_done.cancelled().await;
        self.shared.writer_done.cancelled().await;
    }

    /// The end, once there is one.
    pub(crate) fn end(&self) -> Option<&End> {
        self.shared.end.get()
    }

    /// Wait for the end.
    pub(crate) async fn ended(&self) -> End {
        self.shared.ended.cancelled().await;
        self.shared.end.get().expect("ended is cancelled only after the end is set").clone()
    }
}

impl Drop for CastConnection {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// The error a request gets when the connection is over.
pub(crate) fn ended_error(end: &End) -> RendererError {
    match end {
        End::Lost(reason) => RendererError::Connection(reason.clone()),
        End::Closed => RendererError::Connection("the connection was closed".to_string()),
    }
}

fn panic_ends(shared: &Arc<Shared>) -> impl FnOnce(bae_util::task_failure::Bug) + Send + 'static {
    let shared = shared.clone();
    move |bug| shared.lose(bug.to_string())
}

enum KeepAlive {
    /// Nothing is owed: ping at this moment if nothing arrives.
    Quiet { ping_at: Instant },
    /// A PING was sent: the receiver is lost at this moment if nothing arrives.
    Pinged { lost_at: Instant },
}

impl KeepAlive {
    fn after_a_frame() -> Self {
        KeepAlive::Quiet { ping_at: Instant::now() + PING_AFTER }
    }

    fn due(&self) -> Instant {
        match self {
            KeepAlive::Quiet { ping_at } => *ping_at,
            KeepAlive::Pinged { lost_at } => *lost_at,
        }
    }
}

async fn read_loop<R: AsyncRead + Unpin>(
    read_half: R,
    mut handler: Handler,
    jobs: mpsc::UnboundedSender<Job>,
    shared: Arc<Shared>,
) {
    let _done = shared.reader_done.clone().drop_guard();
    let mut frames = FrameReader::new(read_half);
    let mut keep_alive = KeepAlive::after_a_frame();
    let queue = |request: Request<'_>| {
        let bytes = encode_frame(&request.into_frame("sender-0", "receiver-0"))
            .expect("a heartbeat fits a frame");
        jobs.send(Job::Frame(bytes)).is_ok()
    };
    loop {
        let frame = tokio::select! {
            biased;
            () = shared.stop_reader.cancelled() => return,
            frame = frames.next() => frame,
            () = tokio::time::sleep_until(keep_alive.due()) => {
                match keep_alive {
                    KeepAlive::Quiet { .. } => {
                        if !queue(Request::Ping) {
                            shared.lose("the writer is gone".to_string());
                            return;
                        }
                        keep_alive = KeepAlive::Pinged { lost_at: Instant::now() + LOST_AFTER_PING };
                        continue;
                    }
                    KeepAlive::Pinged { .. } => {
                        shared.lose(format!(
                            "the receiver sent nothing for {} s after a ping",
                            LOST_AFTER_PING.as_secs()
                        ));
                        return;
                    }
                }
            }
        };
        keep_alive = KeepAlive::after_a_frame();
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                shared.lose("closed by the receiver".to_string());
                return;
            }
            Err(error) => {
                shared.lose(wire_failure(&error));
                return;
            }
        };
        let inbound = match Inbound::from_frame(&frame) {
            Ok(inbound) => inbound,
            Err(error) => {
                shared.lose(CauseChain(&error).to_string());
                return;
            }
        };
        match inbound {
            Inbound::Ping => {
                if !queue(Request::Pong) {
                    shared.lose("the writer is gone".to_string());
                    return;
                }
            }
            Inbound::Pong => {}
            Inbound::Ignored { namespace, kind } => {
                debug!(%namespace, ?kind, "cast frame dropped: bae doesn't use it");
            }
            other => {
                if let Err(reason) = handler(other) {
                    shared.lose(reason);
                    return;
                }
            }
        }
    }
}

fn wire_failure(error: &WireError) -> String {
    match error {
        WireError::Io(io) if io.kind() == std::io::ErrorKind::UnexpectedEof => {
            "closed by the receiver".to_string()
        }
        WireError::Io(_) | WireError::Truncated => format!("reading failed: {}", CauseChain(error)),
        _ => format!("the receiver sent a frame bae refuses: {}", CauseChain(error)),
    }
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut write_half: W,
    mut queue: mpsc::UnboundedReceiver<Job>,
    shared: Arc<Shared>,
) {
    let _done = shared.writer_done.clone().drop_guard();
    loop {
        let job = tokio::select! {
            biased;
            () = shared.stop_writer.cancelled() => return,
            job = queue.recv() => job,
        };
        let Some(job) = job else { return };
        let written = match job {
            Job::Frame(bytes) => tokio::time::timeout(WRITE_TIMEOUT, async {
                write_half.write_all(&bytes).await?;
                write_half.flush().await
            })
            .await
            .map_err(|_elapsed| {
                "the receiver didn't take a write within the time allowed".to_string()
            })
            .and_then(|written| written.map_err(|e| format!("writing failed: {e}"))),
            Job::Shutdown => {
                // The end is already Closed, so a failure here changes nothing.
                match tokio::time::timeout(WRITE_TIMEOUT, write_half.shutdown()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => debug!("closing the cast stream failed: {error}"),
                    Err(_) => debug!("closing the cast stream took too long"),
                }
                return;
            }
        };
        if let Err(reason) = written {
            shared.lose(reason);
            return;
        }
    }
}

/// The TLS settings for a Cast receiver. Receivers present self-signed
/// certificates, so any certificate is accepted, while the handshake's
/// signatures are still checked. No key log is set, whatever the environment
/// holds.
pub(crate) fn tls_config() -> ClientConfig {
    let provider = Arc::new(aws_lc_rs::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("the provider supports the safe default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyCertificate { provider }))
        .with_no_client_auth();
    config.key_log = Arc::new(rustls::NoKeyLog);
    config
}

#[derive(Debug)]
struct AnyCertificate {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
