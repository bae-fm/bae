//! Crash reporting stays in the platform apps because crash handlers, native
//! crash capture, and symbol upload are platform-specific. Logs and telemetry
//! live here so Rust and host-originated events share schema, redaction,
//! batching, retry, and transport.
//!
//! The Datadog logs transport sends directly to the client intake endpoint used
//! by Datadog's browser, iOS, and Android SDKs: `browser-intake-.../api/v2/logs`
//! with a client token. Client tokens are public intake credentials for
//! end-user apps; `DD_API_KEY` is never shipped in bae. baeium configures
//! no-op crash reporting and no-op diagnostics.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock, RwLock},
    time::Duration,
};

use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE},
    StatusCode, Url,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{field::Visit, Event, Subscriber};
use tracing_subscriber::{layer::Context, Layer};
use uuid::Uuid;

const BATCH_SIZE: usize = 50;
const MAX_BUFFERED_EVENTS: usize = 1_000;
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_ATTEMPTS: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(250);
const DATADOG_ORIGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

static GLOBAL_DIAGNOSTICS: OnceLock<RwLock<Diagnostics>> = OnceLock::new();

type DiagnosticFields = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticsConfig {
    pub enabled: bool,
    pub datadog_site: String,
    pub client_token: String,
    pub source: String,
    pub service: String,
    pub environment: String,
    pub app_version: String,
    pub edition: String,
    pub git_commit: String,
}

impl DiagnosticsConfig {
    pub fn sends_events(&self) -> bool {
        self.enabled
            && !self.datadog_site.trim().is_empty()
            && !self.client_token.trim().is_empty()
            && !self.source.trim().is_empty()
            && !self.service.trim().is_empty()
            && !self.environment.trim().is_empty()
            && !self.app_version.trim().is_empty()
            && !self.edition.trim().is_empty()
            && !self.git_commit.trim().is_empty()
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            datadog_site: String::new(),
            client_token: String::new(),
            source: String::new(),
            service: String::new(),
            environment: String::new(),
            app_version: String::new(),
            edition: String::new(),
            git_commit: String::new(),
        }
    }

    fn metadata(&self) -> AppDiagnosticMetadata {
        AppDiagnosticMetadata {
            service: self.service.clone(),
            environment: self.environment.clone(),
            app_version: self.app_version.clone(),
            edition: self.edition.clone(),
            git_commit: self.git_commit.clone(),
        }
    }

    fn intake_url(&self) -> Result<Url, DiagnosticsError> {
        validate_datadog_site(&self.datadog_site)?;
        let mut url = Url::parse(&format!(
            "https://browser-intake-{}/api/v2/logs",
            self.datadog_site
        ))
        .map_err(|source| DiagnosticsError::InvalidUrl {
            site: self.datadog_site.clone(),
            detail: source.to_string(),
        })?;
        url.query_pairs_mut().append_pair("ddsource", &self.source);
        Ok(url)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub kind: DiagnosticKind,
    pub name: String,
    pub level: DiagnosticLevel,
    pub target: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub fields: DiagnosticFields,
    #[serde(flatten)]
    pub app: AppDiagnosticMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppDiagnosticMetadata {
    pub service: String,
    #[serde(rename = "env")]
    pub environment: String,
    #[serde(rename = "version")]
    pub app_version: String,
    pub edition: String,
    pub git_commit: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    Log,
    Telemetry,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<DiagnosticsInner>,
}

impl fmt::Debug for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Diagnostics").finish_non_exhaustive()
    }
}

enum DiagnosticsInner {
    Noop,
    Worker {
        tx: mpsc::UnboundedSender<WorkerMessage>,
        app: AppDiagnosticMetadata,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct NoopDiagnostics;

impl NoopDiagnostics {
    pub fn log(
        self,
        level: DiagnosticLevel,
        target: impl Into<String>,
        message: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let _ = (
            level,
            target.into(),
            message.into(),
            fields.into_iter().count(),
        );
    }

    pub fn event(
        self,
        name: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let _ = (name.into(), fields.into_iter().count());
    }
}

impl Diagnostics {
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(DiagnosticsInner::Noop),
        }
    }

    pub fn configure(config: DiagnosticsConfig) -> Result<Self, DiagnosticsError> {
        if !config.sends_events() {
            return Ok(Self::noop());
        }
        Self::with_transport(config, Arc::new(DatadogTransport::new()))
    }

    pub fn install_global(config: DiagnosticsConfig) -> Result<Self, DiagnosticsError> {
        let diagnostics = Self::configure(config)?;
        let lock = GLOBAL_DIAGNOSTICS.get_or_init(|| RwLock::new(Self::noop()));
        let mut guard = lock.write().map_err(|_| DiagnosticsError::GlobalPoisoned)?;
        *guard = diagnostics.clone();
        Ok(diagnostics)
    }

    pub fn global() -> Self {
        let lock = GLOBAL_DIAGNOSTICS.get_or_init(|| RwLock::new(Self::noop()));
        match lock.read() {
            Ok(guard) => guard.clone(),
            Err(_) => Self::noop(),
        }
    }

    fn with_transport(
        config: DiagnosticsConfig,
        transport: Arc<dyn DiagnosticsTransport>,
    ) -> Result<Self, DiagnosticsError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let app = config.metadata();
        let worker = DiagnosticsWorker::new(config, transport, rx);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(DiagnosticsError::BuildRuntime)?;
        std::thread::Builder::new()
            .name("bae-diagnostics".to_string())
            .spawn(move || runtime.block_on(worker.run()))
            .map_err(DiagnosticsError::SpawnWorker)?;

        Ok(Self {
            inner: Arc::new(DiagnosticsInner::Worker { tx, app }),
        })
    }

    pub fn log(
        &self,
        level: DiagnosticLevel,
        target: impl Into<String>,
        message: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let app = match self.app_metadata() {
            Some(app) => app,
            None => return,
        };
        self.emit(DiagnosticEvent {
            kind: DiagnosticKind::Log,
            name: "log".to_string(),
            level,
            target: target.into(),
            message: redact_text(&message.into()),
            timestamp: Utc::now(),
            fields: sanitize_fields(fields),
            app,
        });
    }

    pub fn event(
        &self,
        name: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let name = name.into();
        let app = match self.app_metadata() {
            Some(app) => app,
            None => return,
        };
        self.emit(DiagnosticEvent {
            kind: DiagnosticKind::Telemetry,
            name: redact_text(&name),
            level: DiagnosticLevel::Info,
            target: "telemetry".to_string(),
            message: redact_text(&name),
            timestamp: Utc::now(),
            fields: sanitize_fields(fields),
            app,
        });
    }

    pub async fn flush(&self) -> bool {
        let DiagnosticsInner::Worker { tx, .. } = self.inner.as_ref() else {
            return true;
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if tx.send(WorkerMessage::Flush(reply_tx)).is_err() {
            return false;
        }
        reply_rx.await.unwrap_or(false)
    }

    fn emit(&self, event: DiagnosticEvent) {
        let DiagnosticsInner::Worker { tx, .. } = self.inner.as_ref() else {
            return;
        };
        let _ = tx.send(WorkerMessage::Event(event));
    }

    fn app_metadata(&self) -> Option<AppDiagnosticMetadata> {
        match self.inner.as_ref() {
            DiagnosticsInner::Noop => None,
            DiagnosticsInner::Worker { app, .. } => Some(app.clone()),
        }
    }
}

enum WorkerMessage {
    Event(DiagnosticEvent),
    Flush(oneshot::Sender<bool>),
}

struct DiagnosticsWorker {
    config: DiagnosticsConfig,
    transport: Arc<dyn DiagnosticsTransport>,
    rx: mpsc::UnboundedReceiver<WorkerMessage>,
    buffered: VecDeque<DiagnosticEvent>,
}

impl DiagnosticsWorker {
    fn new(
        config: DiagnosticsConfig,
        transport: Arc<dyn DiagnosticsTransport>,
        rx: mpsc::UnboundedReceiver<WorkerMessage>,
    ) -> Self {
        Self {
            config,
            transport,
            rx,
            buffered: VecDeque::new(),
        }
    }

    async fn run(mut self) {
        let mut flush_interval = tokio::time::interval(FLUSH_INTERVAL);
        loop {
            tokio::select! {
                Some(message) = self.rx.recv() => {
                    match message {
                        WorkerMessage::Event(event) => {
                            self.push(event);
                            if self.buffered.len() >= BATCH_SIZE {
                                let _sent = self.flush_buffer().await;
                            }
                        }
                        WorkerMessage::Flush(reply) => {
                            let sent = self.flush_buffer().await;
                            let _ = reply.send(sent);
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    let _sent = self.flush_buffer().await;
                }
                else => break,
            }
        }
    }

    fn push(&mut self, event: DiagnosticEvent) {
        if self.buffered.len() == MAX_BUFFERED_EVENTS {
            self.buffered.pop_front();
        }
        self.buffered.push_back(event);
    }

    async fn flush_buffer(&mut self) -> bool {
        if self.buffered.is_empty() {
            return true;
        }

        let batch_len = self.buffered.len().min(BATCH_SIZE);
        let batch: Vec<_> = self.buffered.drain(..batch_len).collect();
        let request = match DatadogRequest::build(&self.config, &batch) {
            Ok(request) => request,
            Err(_) => {
                self.restore_front(batch);
                return false;
            }
        };

        match send_with_retry(self.transport.as_ref(), request).await {
            Ok(()) => true,
            Err(_) => {
                self.restore_front(batch);
                false
            }
        }
    }

    fn restore_front(&mut self, mut batch: Vec<DiagnosticEvent>) {
        while let Some(event) = batch.pop() {
            self.buffered.push_front(event);
        }
        while self.buffered.len() > MAX_BUFFERED_EVENTS {
            self.buffered.pop_front();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatadogRequest {
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl DatadogRequest {
    pub fn build(
        config: &DiagnosticsConfig,
        events: &[DiagnosticEvent],
    ) -> Result<Self, DiagnosticsError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "DD-API-KEY",
            HeaderValue::from_str(&config.client_token).map_err(DiagnosticsError::InvalidHeader)?,
        );
        headers.insert(
            "DD-EVP-ORIGIN",
            HeaderValue::from_str(&config.source).map_err(DiagnosticsError::InvalidHeader)?,
        );
        headers.insert(
            "DD-EVP-ORIGIN-VERSION",
            HeaderValue::from_static(DATADOG_ORIGIN_VERSION),
        );
        headers.insert(
            "DD-REQUEST-ID",
            HeaderValue::from_str(&Uuid::new_v4().to_string())
                .map_err(DiagnosticsError::InvalidHeader)?,
        );

        Ok(Self {
            url: config.intake_url()?,
            headers,
            body: serde_json::to_vec(events).map_err(DiagnosticsError::Serialize)?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostics worker failed to start")]
    SpawnWorker(#[source] std::io::Error),
    #[error("diagnostics runtime failed to start")]
    BuildRuntime(#[source] std::io::Error),
    #[error("diagnostics global state is poisoned")]
    GlobalPoisoned,
    #[error("diagnostics Datadog site is invalid: {0}")]
    InvalidSite(String),
    #[error("diagnostics Datadog URL is invalid for site {site}: {detail}")]
    InvalidUrl { site: String, detail: String },
    #[error("diagnostics HTTP header is invalid")]
    InvalidHeader(#[source] reqwest::header::InvalidHeaderValue),
    #[error("diagnostic event serialization failed")]
    Serialize(#[source] serde_json::Error),
    #[error("diagnostics transport failed")]
    Transport(#[source] reqwest::Error),
    #[error("diagnostics intake rejected the batch with HTTP {0}")]
    Status(StatusCode),
}

pub trait DiagnosticsTransport: Send + Sync {
    fn send<'a>(
        &'a self,
        request: DatadogRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), DiagnosticsError>> + Send + 'a>>;
}

#[derive(Debug)]
pub struct DatadogTransport {
    client: reqwest::Client,
}

impl DatadogTransport {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl DiagnosticsTransport for DatadogTransport {
    fn send<'a>(
        &'a self,
        request: DatadogRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), DiagnosticsError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .client
                .post(request.url)
                .headers(request.headers)
                .body(request.body)
                .send()
                .await
                .map_err(DiagnosticsError::Transport)?;

            if response.status().is_success() {
                Ok(())
            } else {
                Err(DiagnosticsError::Status(response.status()))
            }
        })
    }
}

async fn send_with_retry(
    transport: &dyn DiagnosticsTransport,
    request: DatadogRequest,
) -> Result<(), DiagnosticsError> {
    let mut last_error = None;
    for attempt in 0..RETRY_ATTEMPTS {
        match transport.send(request.clone()).await {
            Ok(()) => return Ok(()),
            Err(e) if should_retry(&e) => {
                last_error = Some(e);
                if attempt + 1 < RETRY_ATTEMPTS {
                    tokio::time::sleep(RETRY_DELAY).await;
                }
            }
            Err(e) => return Err(e),
        }
    }
    match last_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub fn should_retry(error: &DiagnosticsError) -> bool {
    match error {
        DiagnosticsError::Transport(_) => true,
        DiagnosticsError::Status(status) => {
            status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
        }
        DiagnosticsError::SpawnWorker(_)
        | DiagnosticsError::BuildRuntime(_)
        | DiagnosticsError::GlobalPoisoned
        | DiagnosticsError::InvalidSite(_)
        | DiagnosticsError::InvalidUrl { .. }
        | DiagnosticsError::InvalidHeader(_)
        | DiagnosticsError::Serialize(_) => false,
    }
}

pub fn tracing_layer() -> DiagnosticsTracingLayer {
    DiagnosticsTracingLayer
}

#[derive(Clone, Debug)]
pub struct DiagnosticsTracingLayer;

impl<S> Layer<S> for DiagnosticsTracingLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::new();
        event.record(&mut visitor);
        let metadata = event.metadata();
        let message = visitor
            .message
            .unwrap_or_else(|| metadata.name().to_string());
        Diagnostics::global().log(
            DiagnosticLevel::from_tracing(metadata.level()),
            metadata.target(),
            message,
            visitor.fields,
        );
    }
}

impl DiagnosticLevel {
    fn from_tracing(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

struct EventVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl EventVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: Vec::new(),
        }
    }
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

fn sanitize_fields(fields: impl IntoIterator<Item = (String, String)>) -> DiagnosticFields {
    fields
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim();
            if field_allowed(key) {
                Some((key.to_string(), redact_text(&value)))
            } else {
                None
            }
        })
        .collect()
}

fn field_allowed(key: &str) -> bool {
    matches!(
        key,
        "action"
            | "album_count"
            | "cloud_provider"
            | "count"
            | "duration_ms"
            | "error_code"
            | "error_kind"
            | "library_state"
            | "operation"
            | "position_ms"
            | "provider"
            | "release_id_kind"
            | "screen"
            | "source"
            | "status"
            | "track_count"
    )
}

fn redact_text(value: &str) -> String {
    let value = path_regex().replace_all(value, "[redacted-path]");
    let value = uuid_regex().replace_all(&value, "[redacted-id]");
    secret_regex()
        .replace_all(&value, "[redacted-secret]")
        .into_owned()
}

fn path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(/[A-Za-z0-9._ -]+(?:/[A-Za-z0-9._ -]+)+|[A-Za-z]:\\[^\s]+)")
            .expect("path redaction regex compiles")
    })
}

fn uuid_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
            .expect("UUID redaction regex compiles")
    })
}

fn secret_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b[A-Za-z0-9_-]{32,}\b").expect("secret redaction regex compiles")
    })
}

fn validate_datadog_site(site: &str) -> Result<(), DiagnosticsError> {
    let valid = !site.is_empty()
        && site
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && site.contains('.');
    if valid {
        Ok(())
    } else {
        Err(DiagnosticsError::InvalidSite(site.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::{Mutex, MutexGuard};
    use tracing_subscriber::prelude::*;

    fn config() -> DiagnosticsConfig {
        DiagnosticsConfig {
            enabled: true,
            datadog_site: "datadoghq.com".to_string(),
            client_token: "client-token".to_string(),
            source: "ios".to_string(),
            service: "bae".to_string(),
            environment: "test".to_string(),
            app_version: "1.2.3".to_string(),
            edition: "bae".to_string(),
            git_commit: "abc123".to_string(),
        }
    }

    fn event() -> DiagnosticEvent {
        DiagnosticEvent {
            kind: DiagnosticKind::Log,
            name: "log".to_string(),
            level: DiagnosticLevel::Warn,
            target: "target".to_string(),
            message: "message".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-06-20T00:00:00Z")
                .expect("test timestamp parses")
                .with_timezone(&Utc),
            fields: BTreeMap::from([("operation".to_string(), "scan".to_string())]),
            app: config().metadata(),
        }
    }

    #[test]
    fn event_serialization_contains_schema_and_app_metadata() {
        let json = serde_json::to_value(event()).expect("event serializes");

        assert_eq!(json["kind"], "log");
        assert_eq!(json["name"], "log");
        assert_eq!(json["level"], "warn");
        assert_eq!(json["target"], "target");
        assert_eq!(json["message"], "message");
        assert_eq!(json["service"], "bae");
        assert_eq!(json["env"], "test");
        assert_eq!(json["version"], "1.2.3");
        assert_eq!(json["edition"], "bae");
        assert_eq!(json["git_commit"], "abc123");
        assert_eq!(json["fields"]["operation"], "scan");
    }

    #[test]
    fn redaction_drops_unapproved_fields_and_redacts_sensitive_values() {
        let fields = sanitize_fields([
            ("operation".to_string(), "scan".to_string()),
            (
                "path".to_string(),
                "/Users/person/Music/file.flac".to_string(),
            ),
            (
                "status".to_string(),
                "stored at /Users/person/Music/file.flac".to_string(),
            ),
            (
                "error_code".to_string(),
                "abcd1234abcd1234abcd1234abcd1234".to_string(),
            ),
        ]);

        assert_eq!(fields.get("operation").map(String::as_str), Some("scan"));
        assert!(!fields.contains_key("path"));
        assert_eq!(
            fields.get("status").map(String::as_str),
            Some("stored at [redacted-path]")
        );
        assert_eq!(
            fields.get("error_code").map(String::as_str),
            Some("[redacted-secret]")
        );
    }

    #[test]
    fn no_op_diagnostics_drops_events() {
        let diagnostics = Diagnostics::noop();
        diagnostics.log(
            DiagnosticLevel::Info,
            "target",
            "message",
            [("operation".to_string(), "scan".to_string())],
        );
        diagnostics.event("opened", [("screen".to_string(), "library".to_string())]);
    }

    #[test]
    fn datadog_request_uses_client_intake_shape() {
        let request = DatadogRequest::build(&config(), &[event()]).expect("request builds");

        assert_eq!(
            request.url.as_str(),
            "https://browser-intake-datadoghq.com/api/v2/logs?ddsource=ios"
        );
        assert_eq!(
            request
                .headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            request
                .headers
                .get("DD-API-KEY")
                .and_then(|v| v.to_str().ok()),
            Some("client-token")
        );
        assert_eq!(
            request
                .headers
                .get("DD-EVP-ORIGIN")
                .and_then(|v| v.to_str().ok()),
            Some("ios")
        );
        assert!(request.headers.contains_key("DD-EVP-ORIGIN-VERSION"));
        assert!(request.headers.contains_key("DD-REQUEST-ID"));
        let body: serde_json::Value =
            serde_json::from_slice(&request.body).expect("request body is JSON");
        assert_eq!(body.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn retry_decision_retries_transient_failures_only() {
        assert!(should_retry(&DiagnosticsError::Status(
            StatusCode::INTERNAL_SERVER_ERROR
        )));
        assert!(should_retry(&DiagnosticsError::Status(
            StatusCode::TOO_MANY_REQUESTS
        )));
        assert!(!should_retry(&DiagnosticsError::Status(
            StatusCode::BAD_REQUEST
        )));
        assert!(!should_retry(&DiagnosticsError::InvalidSite(
            "bad/site".to_string()
        )));
    }

    #[tokio::test]
    async fn batching_flushes_events_through_transport() {
        let transport = Arc::new(RecordingTransport::new(vec![Ok(())]));
        let diagnostics =
            Diagnostics::with_transport(config(), transport.clone()).expect("diagnostics starts");
        diagnostics.log(
            DiagnosticLevel::Info,
            "target",
            "message",
            [("operation".to_string(), "scan".to_string())],
        );

        assert!(diagnostics.flush().await);
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body is JSON");
        assert_eq!(body.as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn failed_flush_keeps_events_buffered_for_later_retry() {
        let transport = Arc::new(RecordingTransport::new(vec![
            Err(DiagnosticsError::Status(StatusCode::BAD_REQUEST)),
            Ok(()),
        ]));
        let diagnostics =
            Diagnostics::with_transport(config(), transport.clone()).expect("diagnostics starts");
        diagnostics.event("opened", [("screen".to_string(), "library".to_string())]);

        assert!(!diagnostics.flush().await);
        assert!(diagnostics.flush().await);
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn tracing_layer_maps_events_to_diagnostics() {
        let transport = Arc::new(RecordingTransport::new(vec![Ok(())]));
        let diagnostics =
            Diagnostics::with_transport(config(), transport.clone()).expect("diagnostics starts");
        install_global_for_test(diagnostics.clone());

        tracing::subscriber::with_default(
            tracing_subscriber::registry().with(tracing_layer()),
            || {
                tracing::warn!(
                    target: "scan",
                    operation = "import",
                    path = "/Users/person/Music/file.flac",
                    "release {} failed",
                    "123e4567-e89b-12d3-a456-426614174000"
                );
            },
        );

        assert!(diagnostics.flush().await);
        let requests = transport.requests();
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body is JSON");
        let event = &body[0];
        assert_eq!(event["level"], "warn");
        assert_eq!(event["target"], "scan");
        assert_eq!(event["fields"]["operation"], "import");
        assert!(event["fields"].get("path").is_none());
        assert_eq!(event["message"], "release [redacted-id] failed");
    }

    fn install_global_for_test(diagnostics: Diagnostics) {
        let lock = GLOBAL_DIAGNOSTICS.get_or_init(|| RwLock::new(Diagnostics::noop()));
        let mut guard = lock.write().expect("global diagnostics lock is available");
        *guard = diagnostics;
    }

    struct RecordingTransport {
        requests: Mutex<Vec<DatadogRequest>>,
        outcomes: Mutex<VecDeque<Result<(), DiagnosticsError>>>,
    }

    impl RecordingTransport {
        fn new(outcomes: Vec<Result<(), DiagnosticsError>>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                outcomes: Mutex::new(outcomes.into()),
            }
        }

        fn requests(&self) -> Vec<DatadogRequest> {
            self.requests_guard().clone()
        }

        fn requests_guard(&self) -> MutexGuard<'_, Vec<DatadogRequest>> {
            self.requests.lock().expect("requests mutex is available")
        }

        fn outcomes_guard(&self) -> MutexGuard<'_, VecDeque<Result<(), DiagnosticsError>>> {
            self.outcomes.lock().expect("outcomes mutex is available")
        }
    }

    impl DiagnosticsTransport for RecordingTransport {
        fn send<'a>(
            &'a self,
            request: DatadogRequest,
        ) -> Pin<Box<dyn Future<Output = Result<(), DiagnosticsError>> + Send + 'a>> {
            Box::pin(async move {
                self.requests_guard().push(request);
                self.outcomes_guard()
                    .pop_front()
                    .expect("test provides a transport outcome")
            })
        }
    }
}
