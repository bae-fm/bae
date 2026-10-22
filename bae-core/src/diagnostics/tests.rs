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
    Arc::new(coven::FixedClock::new(
        (DateTime::parse_from_rfc3339("2026-06-20T00:00:00Z")
            .expect("test timestamp parses")
            .with_timezone(&Utc))
        .into(),
    ))
}

fn ids() -> IdSourceRef {
    Arc::new(coven::SequentialIds::new())
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
        timestamp: clock().now().into(),
        device_id: Some("device-abc".to_string()),
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
        Diagnostics::configure(DiagnosticsConfig::Enabled(incomplete), clock(), ids()),
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
    let diagnostics = Diagnostics::with_transport(config(), clock(), ids(), transport.clone())
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
    let diagnostics = Diagnostics::with_transport(config(), clock(), ids(), transport.clone())
        .expect("diagnostics starts");
    // Once enriched, two events from one instance share the session id and
    // carry the device id.
    diagnostics.set_device_id("device-abc".to_string());
    diagnostics.event(TelemetryEvent::AppStarted {});
    diagnostics.event(TelemetryEvent::AppStarted {});
    diagnostics.flush().await.expect("flush succeeds");

    let body: Vec<DiagnosticEvent> = serde_json::from_slice(&transport.requests()[0].body)
        .expect("request body is a diagnostic-event array");
    assert_eq!(body.len(), 2);
    assert_eq!(body[0].device_id.as_deref(), Some("device-abc"));
    assert_eq!(body[1].device_id.as_deref(), Some("device-abc"));
    assert_eq!(body[0].session_id, body[1].session_id);
}

#[tokio::test]
async fn device_id_enrichment_is_absent_before_and_present_after() {
    let transport = Arc::new(RecordingTransport::new(vec![Ok(()), Ok(())]));
    let diagnostics = Diagnostics::with_transport(config(), clock(), ids(), transport.clone())
        .expect("diagnostics starts");
    // Before enrichment: the field is absent from the wire, not a placeholder.
    diagnostics.event(TelemetryEvent::AppStarted {});
    diagnostics.flush().await.expect("first flush succeeds");
    diagnostics.set_device_id("device-abc".to_string());
    // After enrichment: the field carries the id.
    diagnostics.event(TelemetryEvent::AppStarted {});
    diagnostics.flush().await.expect("second flush succeeds");

    let requests = transport.requests();
    let before: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("request body is JSON");
    assert!(
        before[0].get("device_id").is_none(),
        "pre-enrichment event omits device_id entirely"
    );
    let after: serde_json::Value =
        serde_json::from_slice(&requests[1].body).expect("request body is JSON");
    assert_eq!(after[0]["device_id"], "device-abc");
}

#[tokio::test]
async fn second_device_id_set_is_ignored() {
    let transport = Arc::new(RecordingTransport::new(vec![Ok(())]));
    let diagnostics = Diagnostics::with_transport(config(), clock(), ids(), transport.clone())
        .expect("diagnostics starts");
    diagnostics.set_device_id("first".to_string());
    diagnostics.set_device_id("second".to_string());
    diagnostics.event(TelemetryEvent::AppStarted {});
    diagnostics.flush().await.expect("flush succeeds");

    let body: Vec<DiagnosticEvent> = serde_json::from_slice(&transport.requests()[0].body)
        .expect("request body is a diagnostic-event array");
    assert_eq!(body[0].device_id.as_deref(), Some("first"));
}

#[test]
fn no_op_diagnostics_accepts_set_device_id() {
    // Infallible and silent — nothing to assert but that it doesn't panic.
    Diagnostics::noop().set_device_id("device-abc".to_string());
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
        Arc::new(RecordingTransport::new(vec![Ok(())])),
    )
    .expect("diagnostics starts");
    let second = Diagnostics::with_transport(
        config(),
        clock(),
        ids,
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
    let diagnostics = Diagnostics::with_transport(config(), clock(), ids(), transport.clone())
        .expect("diagnostics starts");
    diagnostics.event(TelemetryEvent::AppStarted {});

    assert!(matches!(
        diagnostics.flush().await,
        Err(DiagnosticsError::Status(StatusCode::BAD_REQUEST))
    ));
    diagnostics.flush().await.expect("second flush succeeds");
    assert_eq!(transport.requests().len(), 2);
}

/// A batch that never reached the intake names the refused connection,
/// which reqwest's own text leaves to a cause below it.
#[tokio::test]
async fn a_transport_failure_names_its_cause() {
    let request = DatadogRequest {
        url: Url::parse("http://127.0.0.1:1/api/v2/logs").expect("the test URL parses"),
        headers: HeaderMap::new(),
        body: Vec::new(),
    };
    let error = DatadogTransport::new()
        .expect("the transport builds")
        .send(request)
        .await
        .expect_err("nothing listens on port 1");
    assert!(matches!(error, DiagnosticsError::Transport(_)), "{error}");
    assert!(error.to_string().contains("tcp connect error"), "{error}");
}

/// The intake is reached under bae's outbound HTTP policy, whose user
/// agent the request carries and whose connect and read timeouts keep a
/// stalled intake from holding the worker.
#[tokio::test]
async fn the_transport_sends_under_baes_http_policy() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the intake binds");
    let address = listener.local_addr().expect("the intake has an address");
    let intake = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("the transport connects");
        let mut head = Vec::new();
        let mut chunk = [0u8; 1024];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).await.expect("the intake reads");
            assert!(read > 0, "the transport closed before sending a request");
            head.extend_from_slice(&chunk[..read]);
        }
        stream
            .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .expect("the intake answers");
        String::from_utf8(head).expect("the request head is text")
    });
    let request = DatadogRequest {
        url: Url::parse(&format!("http://{address}/api/v2/logs")).expect("the URL parses"),
        headers: HeaderMap::new(),
        body: Vec::new(),
    };
    DatadogTransport::new()
        .expect("the transport builds")
        .send(request)
        .await
        .expect("the intake accepts the batch");
    let head = intake.await.expect("the intake finishes").to_lowercase();
    let user_agent = format!(
        "user-agent: {}",
        crate::util::http::USER_AGENT.to_lowercase()
    );
    assert!(head.contains(&user_agent), "{head}");
}

/// A bug goes to the platform's crash reporter with its operation and full
/// text; a sink without one logs it only.
#[test]
fn a_bug_reaches_the_crash_reporter_the_platform_handed_in() {
    let reporter = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reporter.clone());

    diagnostics.report_bug("read the upload queue", &"the database is locked");

    assert_eq!(
        reporter.reports(),
        [(
            "read the upload queue".to_string(),
            "the database is locked".to_string()
        )]
    );
    Diagnostics::noop().report_bug("read the upload queue", &"logged only");
}
