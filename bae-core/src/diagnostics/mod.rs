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
//! Local `tracing` (the platform's native log, plus the terminal on the
//! desktop) and the host platform logs keep the full, unredacted detail for
//! debugging; they are no longer forwarded here.
//! Crash reporting stays in the platform apps (native crash capture and symbol
//! upload are platform-specific). A bug core finds — code that broke its own
//! guarantee — goes to the platform's crash reporter through the
//! [`BugReporter`] the platform hands in, with its full text: it is a report
//! for whoever fixes the bug, not telemetry.
//!
//! The transport posts to the client intake endpoint Datadog's browser, iOS, and
//! Android SDKs use (`browser-intake-.../api/v2/logs`) with a client token —
//! a public intake credential for end-user apps. `DD_API_KEY` is never shipped in
//! bae. baeium configures no-op crash reporting and no-op diagnostics.

mod telemetry;

pub use telemetry::*;

use std::{collections::VecDeque, fmt, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use coven::{ClockRef, IdSourceRef};
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
/// Three tries, a quarter second apart: diagnostics are dropped rather than
/// queued, so there is no busy server here worth waiting out.
const RETRY: crate::retry::RetryPolicy =
    crate::retry::RetryPolicy::flat(3, Duration::from_millis(250));
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
/// on first run) and arrives as a set-once enrichment shortly after startup, so
/// events emitted before it lands omit the field (serialized away) rather than
/// carry a placeholder. `session_id` is fresh per `Diagnostics` construction
/// (per launch). Both are locally-minted random ids that resolve to nothing
/// outside this library — they identify a stream, not a real-world entity — so
/// they live here on core's own event, never on the host-built
/// `AppDiagnosticMetadata`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub name: String,
    #[serde(rename = "status")]
    pub level: DiagnosticLevel,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
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

/// The platform's crash reporter, as core reports a bug to it: one non-fatal
/// event naming the operation that broke and the error's full text.
pub trait BugReporter: Send + Sync {
    fn report(&self, operation: &str, detail: &str);
}

#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<DiagnosticsInner>,
    /// Where bugs go beside the log. `None` for the sink a test or preview
    /// builds, which logs them only.
    bugs: Option<Arc<dyn BugReporter>>,
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
        /// Minted once, here, per construction — one value for this launch.
        session_id: String,
    },
}

impl Diagnostics {
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(DiagnosticsInner::Noop),
            bugs: None,
        }
    }

    pub fn configure(
        config: DiagnosticsConfig,
        clock: ClockRef,
        ids: IdSourceRef,
    ) -> Result<Self, DiagnosticsError> {
        let DiagnosticsConfig::Enabled(config) = config else {
            return Ok(Self::noop());
        };
        if !config.is_complete() {
            return Err(DiagnosticsError::IncompleteConfig);
        }
        let config = config.normalized();
        let transport = DatadogTransport::new()?;
        Self::with_transport(config, clock, ids, Arc::new(transport))
    }

    pub(crate) fn with_transport(
        config: DatadogDiagnosticsConfig,
        clock: ClockRef,
        ids: IdSourceRef,
        transport: Arc<dyn DiagnosticsTransport>,
    ) -> Result<Self, DiagnosticsError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let app = config.app.clone();
        // One session id per construction (per launch), minted from the same
        // injected id source the worker uses for request ids.
        let session_id = ids.new_id().to_string();
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
                session_id,
            }),
            bugs: None,
        })
    }

    /// This sink, reporting the bugs core finds to `bugs` as well as the log.
    pub fn reporting_bugs_to(self, bugs: Arc<dyn BugReporter>) -> Self {
        Self {
            bugs: Some(bugs),
            ..self
        }
    }

    /// Report a bug — code that broke its own guarantee — to the log at error
    /// level and to the platform's crash reporter as a non-fatal event. The
    /// operation names what broke; `error` is written out in full, causes and
    /// all.
    pub fn report_bug(&self, operation: &str, error: &dyn fmt::Display) {
        let detail = error.to_string();
        tracing::error!(operation, "bug: {detail}");
        if let Some(bugs) = &self.bugs {
            bugs.report(operation, &detail);
        }
    }

    /// Enrich every subsequent event with the stable per-device id, set once
    /// shortly after startup (bootstrap loads the config that mints it). Delivered
    /// as a worker message so the worker — the sole owner of the id — serializes
    /// it with the event stream: events already enqueued keep the absent field,
    /// events after it carry the id. A second call is ignored (a library switch
    /// re-runs bootstrap with the same process-global id). The no-op sink drops
    /// it silently.
    pub fn set_device_id(&self, device_id: String) {
        let DiagnosticsInner::Worker { tx, .. } = self.inner.as_ref() else {
            return;
        };
        if tx.send(WorkerMessage::SetDeviceId(device_id)).is_err() {
            tracing::debug!("diagnostics device id dropped: worker stopped");
        }
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
            session_id,
        } = self.inner.as_ref()
        else {
            return;
        };
        let diagnostic = DiagnosticEvent {
            name: event.name().to_string(),
            level: event.level(),
            message: event.name().to_string(),
            timestamp: chrono::DateTime::<chrono::Utc>::from(clock.now()),
            // The worker stamps the device id when it has one (set-once
            // enrichment); an event enqueued before that ships without it.
            device_id: None,
            session_id: session_id.clone(),
            fields: event.fields(),
            app: app.clone(),
        };
        if tx.send(WorkerMessage::Event(diagnostic)).is_err() {
            tracing::debug!("diagnostics event dropped: worker stopped");
        }
    }

    #[expect(
        clippy::map_err_ignore,
        reason = "a closed worker channel says only that the worker stopped"
    )]
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
    SetDeviceId(String),
    Flush(oneshot::Sender<Result<(), DiagnosticsError>>),
}

struct DiagnosticsWorker {
    config: DatadogDiagnosticsConfig,
    ids: IdSourceRef,
    transport: Arc<dyn DiagnosticsTransport>,
    rx: mpsc::UnboundedReceiver<WorkerMessage>,
    buffered: VecDeque<DiagnosticEvent>,
    /// The stable per-device id, absent until `set_device_id` enriches it. The
    /// worker stamps it onto each event as the event arrives.
    device_id: Option<String>,
}

impl DiagnosticsWorker {
    fn new(
        config: DatadogDiagnosticsConfig,
        ids: IdSourceRef,
        transport: Arc<dyn DiagnosticsTransport>,
        rx: mpsc::UnboundedReceiver<WorkerMessage>,
    ) -> Self {
        Self {
            config,
            ids,
            transport,
            rx,
            buffered: VecDeque::new(),
            device_id: None,
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
                        WorkerMessage::Event(mut event) => {
                            event.device_id = self.device_id.clone();
                            self.push(event);
                            if self.buffered.len() >= BATCH_SIZE {
                                self.record_flush_result().await;
                            }
                        }
                        WorkerMessage::SetDeviceId(device_id) => {
                            if self.device_id.is_some() {
                                tracing::debug!(
                                    "diagnostics device id already set; ignoring repeat"
                                );
                            } else {
                                self.device_id = Some(device_id);
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
            // The batch stays buffered for the next flush; the cause says
            // whether the intake, the network or the batch itself refused it.
            tracing::warn!("diagnostics flush failed: {error}");
        }
    }

    async fn flush_buffer(&mut self) -> Result<(), DiagnosticsError> {
        if self.buffered.is_empty() {
            return Ok(());
        }

        let batch_len = self.buffered.len().min(BATCH_SIZE);
        let batch: Vec<_> = self.buffered.drain(..batch_len).collect();
        let request =
            match DatadogRequest::build(&self.config, &batch, self.ids.new_id().to_string()) {
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
    #[error("diagnostics HTTP client failed to build: {}", crate::util::error_text::CauseChain(.0))]
    BuildClient(#[source] reqwest::Error),
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
    #[error("diagnostics transport failed: {}", crate::util::error_text::CauseChain(.0))]
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
    /// A transport under bae's outbound HTTP policy. It keeps its own client
    /// rather than sharing the app's `Http`: it is built at launch, before
    /// that exists, and runs on the diagnostics worker's own runtime.
    fn new() -> Result<Self, DiagnosticsError> {
        Ok(Self {
            client: crate::util::http::client_builder()
                .build()
                .map_err(DiagnosticsError::BuildClient)?,
        })
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
    // The transport hands back a status and not the response's headers, so
    // a repeat here always takes the flat wait.
    let repeat = |error: &DiagnosticsError| {
        if should_retry(error) {
            crate::retry::Repeat::AfterBackoff
        } else {
            crate::retry::Repeat::Never
        }
    };
    retry_with_backoff_if(RETRY, "diagnostics send", repeat, || {
        transport.send(request.clone())
    })
    .await
}

pub fn should_retry(error: &DiagnosticsError) -> bool {
    match error {
        DiagnosticsError::Transport(_) => true,
        DiagnosticsError::Status(status) => crate::retry::is_transient_status(*status),
        DiagnosticsError::SpawnWorker(_)
        | DiagnosticsError::BuildRuntime(_)
        | DiagnosticsError::BuildClient(_)
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

/// A bug reporter that keeps every report, for a test asserting that a broken
/// guarantee reached the crash reporter.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Default)]
pub struct RecordingBugReporter {
    reports: std::sync::Mutex<Vec<(String, String)>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl RecordingBugReporter {
    /// Every `(operation, detail)` reported so far, in order.
    pub fn reports(&self) -> Vec<(String, String)> {
        self.reports
            .lock()
            .expect("bug reports mutex poisoned")
            .clone()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl BugReporter for RecordingBugReporter {
    fn report(&self, operation: &str, detail: &str) {
        self.reports
            .lock()
            .expect("bug reports mutex poisoned")
            .push((operation.to_string(), detail.to_string()));
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
mod tests;
