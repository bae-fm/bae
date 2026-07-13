//! Shipped telemetry is a closed, typed catalog — never free text.
//!
//! Everything that leaves the device is a [`TelemetryEvent`] (see
//! `telemetry.rs`): a declared name, level, and typed fields, serialized to a
//! [`DiagnosticEvent`] and batched to Datadog by the worker here. Field values
//! go through [`TelemetryValue`], which `String`/`&str` don't implement, so
//! free text, names, paths, and externally-resolvable ids can't enter the
//! payload by construction. There is no scrubbing pass and no allowlist because
//! there is nothing untyped to scrub.
//!
//! Local `tracing` (console/oslog/logcat) and the host platform logs keep the
//! full, unredacted detail for debugging; they are no longer forwarded here.
//! Crash reporting stays in the platform apps (native crash capture and symbol
//! upload are platform-specific).
//!
//! The transport posts to the client intake endpoint Datadog's browser, iOS, and
//! Android SDKs use (`browser-intake-.../api/v2/logs`) with a client token —
//! a public intake credential for end-user apps. `DD_API_KEY` is never shipped in
//! bae. baeium configures no-op crash reporting and no-op diagnostics.

mod telemetry;

pub use telemetry::*;

use std::{collections::VecDeque, fmt, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use coven::{ClockRef, IdRef};
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE},
    StatusCode, Url,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::retry::retry_with_backoff_if;

const BATCH_SIZE: usize = 50;
const MAX_BUFFERED_EVENTS: usize = 1_000;
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_ATTEMPTS: u32 = 3;
/// Flat delay between diagnostics-upload retries. 250ms in production; zero in
/// any test build so a retry-path test spends no real time between attempts
/// (same `test` / `test-utils` seam as `retry::LINEAR_BACKOFF_BASE`).
#[cfg(not(any(test, feature = "test-utils")))]
const RETRY_DELAY: Duration = Duration::from_millis(250);
#[cfg(any(test, feature = "test-utils"))]
const RETRY_DELAY: Duration = Duration::ZERO;
const DATADOG_ORIGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticsConfig {
    Disabled,
    Enabled(DatadogDiagnosticsConfig),
}

impl DiagnosticsConfig {
    pub fn sends_events(&self) -> bool {
        matches!(self, Self::Enabled(config) if config.is_complete())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatadogDiagnosticsConfig {
    pub datadog_site: String,
    pub client_token: String,
    pub source: String,
    pub app: AppDiagnosticMetadata,
}

impl DatadogDiagnosticsConfig {
    fn is_complete(&self) -> bool {
        self.required_values()
            .iter()
            .all(|value| !value.trim().is_empty())
    }

    fn normalized(self) -> Self {
        Self {
            datadog_site: self.datadog_site.trim().to_string(),
            client_token: self.client_token.trim().to_string(),
            source: self.source.trim().to_string(),
            app: AppDiagnosticMetadata {
                service: self.app.service.trim().to_string(),
                environment: self.app.environment.trim().to_string(),
                app_version: self.app.app_version.trim().to_string(),
                edition: self.app.edition.trim().to_string(),
                git_commit: self.app.git_commit.trim().to_string(),
            },
        }
    }

    fn required_values(&self) -> [&str; 8] {
        [
            &self.datadog_site,
            &self.client_token,
            &self.source,
            &self.app.service,
            &self.app.environment,
            &self.app.app_version,
            &self.app.edition,
            &self.app.git_commit,
        ]
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

/// The wire shape of one shipped event. `name` is the telemetry event's declared
/// name; `fields` are its typed values recorded as JSON. `message` duplicates
/// `name` as Datadog's display line, and `level` maps to Datadog's reserved
/// `status` severity attribute. The app metadata is flattened in alongside.
///
/// `device_id` and `session_id` are the correlation keys for reconstructing one
/// user's event stream: `device_id` is stable across launches (config mints it
/// on first run), `session_id` is fresh per `Diagnostics` construction (per
/// launch). Both are locally-minted random ids that resolve to nothing outside
/// this library — they identify a stream, not a real-world entity — so they live
/// here on core's own event, never on the host-built `AppDiagnosticMetadata`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub name: String,
    #[serde(rename = "status")]
    pub level: DiagnosticLevel,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub device_id: String,
    pub session_id: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
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
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
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
        clock: ClockRef,
        /// Stable across launches (config mints it on first run).
        device_id: String,
        /// Minted once, here, per construction — one value for this launch.
        session_id: String,
    },
}

impl Diagnostics {
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(DiagnosticsInner::Noop),
        }
    }

    pub fn configure(
        config: DiagnosticsConfig,
        clock: ClockRef,
        ids: IdRef,
        device_id: String,
    ) -> Result<Self, DiagnosticsError> {
        let DiagnosticsConfig::Enabled(config) = config else {
            return Ok(Self::noop());
        };
        if !config.is_complete() {
            return Err(DiagnosticsError::IncompleteConfig);
        }
        let config = config.normalized();
        Self::with_transport(
            config,
            clock,
            ids,
            device_id,
            Arc::new(DatadogTransport::new()),
        )
    }

    pub(crate) fn with_transport(
        config: DatadogDiagnosticsConfig,
        clock: ClockRef,
        ids: IdRef,
        device_id: String,
        transport: Arc<dyn DiagnosticsTransport>,
    ) -> Result<Self, DiagnosticsError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let app = config.app.clone();
        // One session id per construction (per launch), minted from the same
        // injected id source the worker uses for request ids.
        let session_id = ids.new_id();
        let worker = DiagnosticsWorker::new(config, ids, transport, rx);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(DiagnosticsError::BuildRuntime)?;
        std::thread::Builder::new()
            .name("bae-diagnostics".to_string())
            .spawn(move || runtime.block_on(worker.run()))
            .map_err(DiagnosticsError::SpawnWorker)?;

        Ok(Self {
            inner: Arc::new(DiagnosticsInner::Worker {
                tx,
                app,
                clock,
                device_id,
                session_id,
            }),
        })
    }

    /// Ship one telemetry event. Infallible from the caller's view — telemetry
    /// must never break playback/import/sync. A send failure means the worker
    /// stopped (normal at shutdown); that is logged at `debug` and the event is
    /// dropped, the same logged bail-out the old tracing layer used.
    pub fn event(&self, event: TelemetryEvent) {
        let DiagnosticsInner::Worker {
            tx,
            app,
            clock,
            device_id,
            session_id,
        } = self.inner.as_ref()
        else {
            return;
        };
        let diagnostic = DiagnosticEvent {
            name: event.name().to_string(),
            level: event.level(),
            message: event.name().to_string(),
            timestamp: clock.now(),
            device_id: device_id.clone(),
            session_id: session_id.clone(),
            fields: event.fields(),
            app: app.clone(),
        };
        if tx.send(WorkerMessage::Event(diagnostic)).is_err() {
            tracing::debug!("diagnostics event dropped: worker stopped");
        }
    }

    pub async fn flush(&self) -> Result<(), DiagnosticsError> {
        let DiagnosticsInner::Worker { tx, .. } = self.inner.as_ref() else {
            return Ok(());
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(WorkerMessage::Flush(reply_tx))
            .map_err(|_| DiagnosticsError::WorkerStopped)?;
        reply_rx
            .await
            .map_err(|_| DiagnosticsError::WorkerStopped)?
    }
}

enum WorkerMessage {
    Event(DiagnosticEvent),
    Flush(oneshot::Sender<Result<(), DiagnosticsError>>),
}

struct DiagnosticsWorker {
    config: DatadogDiagnosticsConfig,
    ids: IdRef,
    transport: Arc<dyn DiagnosticsTransport>,
    rx: mpsc::UnboundedReceiver<WorkerMessage>,
    buffered: VecDeque<DiagnosticEvent>,
}

impl DiagnosticsWorker {
    fn new(
        config: DatadogDiagnosticsConfig,
        ids: IdRef,
        transport: Arc<dyn DiagnosticsTransport>,
        rx: mpsc::UnboundedReceiver<WorkerMessage>,
    ) -> Self {
        Self {
            config,
            ids,
            transport,
            rx,
            buffered: VecDeque::new(),
        }
    }

    async fn run(mut self) {
        // `interval`'s first tick completes immediately; start one period out so
        // the worker flushes only every FLUSH_INTERVAL. An immediate tick can land
        // between an enqueued event and an explicit flush(), consuming the
        // transport response that flush was owed and swallowing the error its
        // caller expected to see.
        let mut flush_interval =
            tokio::time::interval_at(tokio::time::Instant::now() + FLUSH_INTERVAL, FLUSH_INTERVAL);
        loop {
            tokio::select! {
                Some(message) = self.rx.recv() => {
                    match message {
                        WorkerMessage::Event(event) => {
                            self.push(event);
                            if self.buffered.len() >= BATCH_SIZE {
                                self.record_flush_result().await;
                            }
                        }
                        WorkerMessage::Flush(reply) => {
                            let result = self.flush_buffer().await;
                            if reply.send(result).is_err() {
                                tracing::debug!("diagnostics flush receiver dropped");
                            }
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    self.record_flush_result().await;
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

    async fn record_flush_result(&mut self) {
        if let Err(error) = self.flush_buffer().await {
            tracing::debug!(%error, "diagnostics flush failed");
        }
    }

    async fn flush_buffer(&mut self) -> Result<(), DiagnosticsError> {
        if self.buffered.is_empty() {
            return Ok(());
        }

        let batch_len = self.buffered.len().min(BATCH_SIZE);
        let batch: Vec<_> = self.buffered.drain(..batch_len).collect();
        let request = match DatadogRequest::build(&self.config, &batch, self.ids.new_id()) {
            Ok(request) => request,
            Err(e) => {
                self.restore_front(batch);
                return Err(e);
            }
        };

        match send_with_retry(self.transport.as_ref(), request).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.restore_front(batch);
                Err(e)
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
        config: &DatadogDiagnosticsConfig,
        events: &[DiagnosticEvent],
        request_id: String,
    ) -> Result<Self, DiagnosticsError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        insert_str_header(&mut headers, "DD-API-KEY", &config.client_token)?;
        insert_str_header(&mut headers, "DD-EVP-ORIGIN", &config.source)?;
        headers.insert(
            "DD-EVP-ORIGIN-VERSION",
            HeaderValue::from_static(DATADOG_ORIGIN_VERSION),
        );
        insert_str_header(&mut headers, "DD-REQUEST-ID", &request_id)?;

        Ok(Self {
            url: config.intake_url()?,
            headers,
            body: serde_json::to_vec(events).map_err(DiagnosticsError::Serialize)?,
        })
    }
}

fn insert_str_header(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), DiagnosticsError> {
    headers.insert(
        name,
        HeaderValue::from_str(value).map_err(DiagnosticsError::InvalidHeader)?,
    );
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostics worker failed to start")]
    SpawnWorker(#[source] std::io::Error),
    #[error("diagnostics runtime failed to start")]
    BuildRuntime(#[source] std::io::Error),
    #[error("diagnostics config is missing required Datadog fields")]
    IncompleteConfig,
    #[error("diagnostics worker stopped")]
    WorkerStopped,
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

#[async_trait::async_trait]
pub trait DiagnosticsTransport: Send + Sync {
    async fn send(&self, request: DatadogRequest) -> Result<(), DiagnosticsError>;
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

#[async_trait::async_trait]
impl DiagnosticsTransport for DatadogTransport {
    async fn send(&self, request: DatadogRequest) -> Result<(), DiagnosticsError> {
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
    }
}

async fn send_with_retry(
    transport: &dyn DiagnosticsTransport,
    request: DatadogRequest,
) -> Result<(), DiagnosticsError> {
    retry_with_backoff_if(
        RETRY_ATTEMPTS,
        "diagnostics send",
        should_retry,
        |_| RETRY_DELAY,
        || transport.send(request.clone()),
    )
    .await
}

pub fn should_retry(error: &DiagnosticsError) -> bool {
    match error {
        DiagnosticsError::Transport(_) => true,
        DiagnosticsError::Status(status) => {
            status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
        }
        DiagnosticsError::SpawnWorker(_)
        | DiagnosticsError::BuildRuntime(_)
        | DiagnosticsError::IncompleteConfig
        | DiagnosticsError::WorkerStopped
        | DiagnosticsError::InvalidSite(_)
        | DiagnosticsError::InvalidUrl { .. }
        | DiagnosticsError::InvalidHeader(_)
        | DiagnosticsError::Serialize(_) => false,
    }
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

/// A transport that records the requests it's handed and replays a fixed queue
/// of outcomes (defaulting to success once the queue drains). Shared by the
/// diagnostics worker tests and the service-level emission tests, which assert
/// the typed events reach the wire.
#[cfg(test)]
pub struct RecordingTransport {
    requests: std::sync::Mutex<Vec<DatadogRequest>>,
    outcomes: std::sync::Mutex<VecDeque<Result<(), DiagnosticsError>>>,
}

#[cfg(test)]
impl RecordingTransport {
    pub fn new(outcomes: Vec<Result<(), DiagnosticsError>>) -> Self {
        Self {
            requests: std::sync::Mutex::new(Vec::new()),
            outcomes: std::sync::Mutex::new(outcomes.into()),
        }
    }

    pub fn requests(&self) -> Vec<DatadogRequest> {
        self.requests
            .lock()
            .expect("requests mutex is available")
            .clone()
    }

    /// Every event name across every recorded request body, in send order.
    pub fn event_names(&self) -> Vec<String> {
        self.requests()
            .iter()
            .flat_map(|request| {
                let body: Vec<DiagnosticEvent> = serde_json::from_slice(&request.body)
                    .expect("recorded request body is a diagnostic-event array");
                body.into_iter().map(|event| event.name)
            })
            .collect()
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl DiagnosticsTransport for RecordingTransport {
    async fn send(&self, request: DatadogRequest) -> Result<(), DiagnosticsError> {
        self.requests
            .lock()
            .expect("requests mutex is available")
            .push(request);
        self.outcomes
            .lock()
            .expect("outcomes mutex is available")
            .pop_front()
            .unwrap_or(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DatadogDiagnosticsConfig {
        DatadogDiagnosticsConfig {
            datadog_site: "datadoghq.com".to_string(),
            client_token: "client-token".to_string(),
            source: "ios".to_string(),
            app: AppDiagnosticMetadata {
                service: "bae".to_string(),
                environment: "test".to_string(),
                app_version: "1.2.3".to_string(),
                edition: "bae".to_string(),
                git_commit: "abc123".to_string(),
            },
        }
    }

    fn clock() -> ClockRef {
        Arc::new(coven::FixedClock(
            DateTime::parse_from_rfc3339("2026-06-20T00:00:00Z")
                .expect("test timestamp parses")
                .with_timezone(&Utc),
        ))
    }

    fn ids() -> IdRef {
        Arc::new(coven::SequentialIdProvider::new("request-id"))
    }

    fn event() -> DiagnosticEvent {
        let telemetry = TelemetryEvent::PlaybackStarted {
            source: PlaybackStartSource::Release,
            track_count: 4,
        };
        DiagnosticEvent {
            name: telemetry.name().to_string(),
            level: telemetry.level(),
            message: telemetry.name().to_string(),
            timestamp: clock().now(),
            device_id: "device-abc".to_string(),
            session_id: "session-xyz".to_string(),
            fields: telemetry.fields(),
            app: config().app,
        }
    }

    fn header_value<'a>(request: &'a DatadogRequest, name: &str) -> &'a str {
        request
            .headers
            .get(name)
            .unwrap_or_else(|| panic!("{name} header is present"))
            .to_str()
            .unwrap_or_else(|e| panic!("{name} header is valid UTF-8: {e}"))
    }

    #[test]
    fn disabled_config_does_not_send_events() {
        assert!(!DiagnosticsConfig::Disabled.sends_events());
    }

    #[test]
    fn enabled_config_requires_every_value() {
        assert!(DiagnosticsConfig::Enabled(config()).sends_events());

        let mut incomplete = config();
        incomplete.client_token = String::new();
        assert!(!DiagnosticsConfig::Enabled(incomplete.clone()).sends_events());
        assert!(matches!(
            Diagnostics::configure(
                DiagnosticsConfig::Enabled(incomplete),
                clock(),
                ids(),
                "device-abc".to_string()
            ),
            Err(DiagnosticsError::IncompleteConfig)
        ));

        let mut whitespace = config();
        whitespace.client_token = "   ".to_string();
        assert!(!DiagnosticsConfig::Enabled(whitespace).sends_events());
    }

    #[test]
    fn enabled_config_normalizes_values_before_sending() {
        let mut config = config();
        config.datadog_site = " datadoghq.com ".to_string();
        config.source = " ios ".to_string();
        config.app.service = " bae ".to_string();

        let normalized = config.normalized();

        assert_eq!(normalized.datadog_site, "datadoghq.com");
        assert_eq!(normalized.source, "ios");
        assert_eq!(normalized.app.service, "bae");
    }

    #[test]
    fn event_serialization_carries_typed_schema_and_app_metadata() {
        let json = serde_json::to_value(event()).expect("event serializes");

        assert_eq!(json["name"], "playback_started");
        // Datadog's reserved severity + display line.
        assert_eq!(json["status"], "info");
        assert_eq!(json["message"], "playback_started");
        // Correlation keys ship as plain top-level fields, not nested.
        assert_eq!(json["device_id"], "device-abc");
        assert_eq!(json["session_id"], "session-xyz");
        // Numbers ship as JSON numbers, enums as snake_case strings.
        assert_eq!(json["fields"]["track_count"], 4);
        assert!(json["fields"]["track_count"].is_number());
        assert_eq!(json["fields"]["source"], "release");
        // Flattened app metadata.
        assert_eq!(json["service"], "bae");
        assert_eq!(json["env"], "test");
        assert_eq!(json["version"], "1.2.3");
        assert_eq!(json["edition"], "bae");
        assert_eq!(json["git_commit"], "abc123");
    }

    #[test]
    fn no_op_diagnostics_drops_events() {
        // Infallible and silent — nothing to assert but that it doesn't panic.
        Diagnostics::noop().event(TelemetryEvent::AppStarted {});
    }

    #[test]
    fn datadog_request_uses_client_intake_shape() {
        let request = DatadogRequest::build(&config(), &[event()], "request-id".to_string())
            .expect("request builds");

        assert_eq!(
            request.url.as_str(),
            "https://browser-intake-datadoghq.com/api/v2/logs?ddsource=ios"
        );
        assert_eq!(
            header_value(&request, CONTENT_TYPE.as_str()),
            "application/json"
        );
        assert_eq!(header_value(&request, "DD-API-KEY"), "client-token");
        assert_eq!(header_value(&request, "DD-EVP-ORIGIN"), "ios");
        assert_eq!(header_value(&request, "DD-REQUEST-ID"), "request-id");
        assert!(request.headers.contains_key("DD-EVP-ORIGIN-VERSION"));
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
        let diagnostics = Diagnostics::with_transport(
            config(),
            clock(),
            ids(),
            "device-abc".to_string(),
            transport.clone(),
        )
        .expect("diagnostics starts");
        diagnostics.event(TelemetryEvent::AppStarted {});

        diagnostics.flush().await.expect("flush succeeds");
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body is JSON");
        assert_eq!(body.as_array().map(Vec::len), Some(1));
        assert_eq!(body[0]["name"], "app_started");
    }

    #[tokio::test]
    async fn events_carry_device_and_session_ids() {
        let transport = Arc::new(RecordingTransport::new(vec![Ok(())]));
        let diagnostics = Diagnostics::with_transport(
            config(),
            clock(),
            ids(),
            "device-abc".to_string(),
            transport.clone(),
        )
        .expect("diagnostics starts");
        // Two events from one instance share the session id and carry the device id.
        diagnostics.event(TelemetryEvent::AppStarted {});
        diagnostics.event(TelemetryEvent::AppStarted {});
        diagnostics.flush().await.expect("flush succeeds");

        let body: Vec<DiagnosticEvent> = serde_json::from_slice(&transport.requests()[0].body)
            .expect("request body is a diagnostic-event array");
        assert_eq!(body.len(), 2);
        assert_eq!(body[0].device_id, "device-abc");
        assert_eq!(body[1].device_id, "device-abc");
        assert_eq!(body[0].session_id, body[1].session_id);
    }

    #[tokio::test]
    async fn distinct_constructions_get_distinct_session_ids() {
        // One shared id source across both constructions: the sequential fake
        // hands each its own value, so the sessions are distinct.
        let ids = ids();
        let first = Diagnostics::with_transport(
            config(),
            clock(),
            ids.clone(),
            "device-abc".to_string(),
            Arc::new(RecordingTransport::new(vec![Ok(())])),
        )
        .expect("diagnostics starts");
        let second = Diagnostics::with_transport(
            config(),
            clock(),
            ids,
            "device-abc".to_string(),
            Arc::new(RecordingTransport::new(vec![Ok(())])),
        )
        .expect("diagnostics starts");

        let DiagnosticsInner::Worker {
            session_id: first_session,
            ..
        } = first.inner.as_ref()
        else {
            panic!("configured diagnostics runs a worker");
        };
        let DiagnosticsInner::Worker {
            session_id: second_session,
            ..
        } = second.inner.as_ref()
        else {
            panic!("configured diagnostics runs a worker");
        };
        assert_ne!(first_session, second_session);
    }

    #[tokio::test]
    async fn failed_flush_keeps_events_buffered_for_later_retry() {
        let transport = Arc::new(RecordingTransport::new(vec![
            Err(DiagnosticsError::Status(StatusCode::BAD_REQUEST)),
            Ok(()),
        ]));
        let diagnostics = Diagnostics::with_transport(
            config(),
            clock(),
            ids(),
            "device-abc".to_string(),
            transport.clone(),
        )
        .expect("diagnostics starts");
        diagnostics.event(TelemetryEvent::AppStarted {});

        assert!(matches!(
            diagnostics.flush().await,
            Err(DiagnosticsError::Status(StatusCode::BAD_REQUEST))
        ));
        diagnostics.flush().await.expect("second flush succeeds");
        assert_eq!(transport.requests().len(), 2);
    }
}
