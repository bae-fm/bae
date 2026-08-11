use super::*;
use crate::import::MetadataSource;
use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SEARCH_OK_EMPTY: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: application/json\r\n",
    "Content-Length: 14\r\n",
    "\r\n",
    "{\"results\":[]}",
);
const RATE_LIMITED: &str = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n";
const UNAUTHORIZED: &str = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
const BAD_REQUEST: &str = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n";

fn discogs_test_guard() -> &'static tokio::sync::Mutex<()> {
    static GUARD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn discogs_response_server(responses: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("test listener should have an address")
    );
    let request_count = Arc::new(AtomicUsize::new(0));
    let counted_requests = request_count.clone();
    tokio::spawn(async move {
        for response in responses {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("test request should connect");
            counted_requests.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 4096];
            let _ = stream
                .read(&mut buffer)
                .await
                .expect("test request should be readable");
            stream
                .write_all(response.as_bytes())
                .await
                .expect("test response should write");
        }
    });
    (url, request_count)
}

fn search_result_with_cover_fields(
    cover_image: Option<&str>,
    thumb: Option<&str>,
) -> DiscogsSearchResult {
    DiscogsSearchResult {
        id: 1,
        title: "Artist Name - Album Title".to_string(),
        year: None,
        format: None,
        country: None,
        label: None,
        catno: None,
        cover_image: cover_image.map(str::to_string),
        thumb: thumb.map(str::to_string),
        master_id: None,
        result_type: "release".to_string(),
    }
}

#[test]
fn search_result_remote_cover_uses_thumb_as_cover_when_cover_image_is_absent() {
    let result = search_result_with_cover_fields(None, Some("https://discogs.example/thumb.jpg"));

    let cover = result.remote_cover().unwrap();

    assert_eq!(cover.url, "https://discogs.example/thumb.jpg");
    assert_eq!(cover.thumbnail_url, "https://discogs.example/thumb.jpg");
    assert_eq!(cover.label, MetadataSource::Discogs.cover_source_label());
    assert_eq!(cover.source, MetadataSource::Discogs);
}

#[test]
fn search_result_remote_cover_uses_cover_as_thumbnail_when_thumb_is_absent() {
    let result = search_result_with_cover_fields(Some("https://discogs.example/full.jpg"), None);

    let cover = result.remote_cover().unwrap();

    assert_eq!(cover.url, "https://discogs.example/full.jpg");
    assert_eq!(cover.thumbnail_url, "https://discogs.example/full.jpg");
    assert_eq!(cover.label, MetadataSource::Discogs.cover_source_label());
    assert_eq!(cover.source, MetadataSource::Discogs);
}

#[test]
fn search_result_remote_cover_is_absent_without_cover_fields() {
    let result = search_result_with_cover_fields(None, None);

    assert!(result.remote_cover().is_none());
}

#[test]
fn observe_signals_only_on_rejection_or_success() {
    let signals = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let recorded = signals.clone();
    let observer: DiscogsValidationObserver = Arc::new(move |sig| {
        recorded.lock().unwrap().push(match sig {
            DiscogsKeySignal::Rejected => "rejected",
            DiscogsKeySignal::Accepted => "accepted",
        });
    });
    let client = DiscogsClient::with_observer("token".to_string(), observer);

    client.observe::<()>(&Ok(()));
    client.observe::<()>(&Err(DiscogsError::InvalidApiKey));
    // A rate limit says nothing about the key, so it must not signal — a
    // transient blip cannot be allowed to reject a good key.
    client.observe::<()>(&Err(DiscogsError::RateLimit));

    assert_eq!(*signals.lock().unwrap(), vec!["accepted", "rejected"]);
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn transport_error_display_does_not_include_discogs_token() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let token = "secret-discogs-token";
    let mut client = DiscogsClient::new(token.to_string());
    client.base_url = "http://127.0.0.1:1".to_string();

    let error = client
        .validate_token(CallPriority::Interactive)
        .await
        .unwrap_err();

    assert!(!error.to_string().contains(token));
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn validate_token_sends_token_in_authorization_header() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let token = "secret-discogs-token";
    let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
    let url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("test listener should have an address")
    );
    let request = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("test request should connect");
        let mut buffer = [0; 4096];
        let read = stream
            .read(&mut buffer)
            .expect("test request should be readable");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .expect("test response should write");
        String::from_utf8(buffer[..read].to_vec()).expect("request should be UTF-8")
    });
    let mut client = DiscogsClient::new(token.to_string());
    client.base_url = url;

    client
        .validate_token(CallPriority::Interactive)
        .await
        .expect("token validation should accept 200 response");

    let request = request.join().expect("test listener should finish");
    let request_line = request
        .lines()
        .next()
        .expect("test request should include a request line");
    assert_eq!(request_line, "GET /database/search?per_page=1 HTTP/1.1");
    assert!(request.contains("authorization: Discogs token=secret-discogs-token\r\n"));
    assert!(!request_line.contains("token=secret-discogs-token"));
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_retries_rate_limit_then_returns_success() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
    let mut client = DiscogsClient::new("token".to_string());
    client.base_url = url;

    let releases = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect("retry should return the successful search response");

    assert!(releases.is_empty());
    assert_eq!(request_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_returns_persistent_rate_limit_after_retry_attempts() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) =
        discogs_response_server(vec![RATE_LIMITED, RATE_LIMITED, RATE_LIMITED]).await;
    let mut client = DiscogsClient::new("token".to_string());
    client.base_url = url;

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("persistent rate limit should fail after retry attempts");

    assert!(matches!(error, DiscogsError::RateLimit));
    assert_eq!(request_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_does_not_retry_invalid_api_key() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) = discogs_response_server(vec![UNAUTHORIZED]).await;
    let mut client = DiscogsClient::new("token".to_string());
    client.base_url = url;

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("invalid API key should fail without retry");

    assert!(matches!(error, DiscogsError::InvalidApiKey));
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

/// A 4xx that isn't one of the carved-out statuses is the server's permanent
/// answer to this request — it must be tried once, not retried. Before the
/// error split it landed in `Request` and was retried like a transport failure.
#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_does_not_retry_client_error() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) = discogs_response_server(vec![BAD_REQUEST]).await;
    let mut client = DiscogsClient::new("token".to_string());
    client.base_url = url;

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("a 400 should fail without retry");

    assert!(
        matches!(error, DiscogsError::Provider(StatusCode::BAD_REQUEST)),
        "expected Provider(400), got {error:?}",
    );
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[test]
fn retry_policy_repeats_only_transient_failures() {
    assert!(should_retry_discogs(&DiscogsError::RateLimit));
    assert!(should_retry_discogs(&DiscogsError::Provider(
        StatusCode::INTERNAL_SERVER_ERROR
    )));
    assert!(should_retry_discogs(&DiscogsError::Provider(
        StatusCode::SERVICE_UNAVAILABLE
    )));
    assert!(!should_retry_discogs(&DiscogsError::Provider(
        StatusCode::BAD_REQUEST
    )));
    assert!(!should_retry_discogs(&DiscogsError::Provider(
        StatusCode::FORBIDDEN
    )));
    assert!(!should_retry_discogs(&DiscogsError::Provider(
        StatusCode::UNPROCESSABLE_ENTITY
    )));
    assert!(!should_retry_discogs(&DiscogsError::InvalidApiKey));
    assert!(!should_retry_discogs(&DiscogsError::NotFound));
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_does_not_retry_not_found() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) = discogs_response_server(vec![NOT_FOUND]).await;
    let mut client = DiscogsClient::new("token".to_string());
    client.base_url = url;

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("not found should fail without retry");

    assert!(matches!(error, DiscogsError::NotFound));
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn search_observer_records_one_signal_after_internal_retry() {
    let _guard = discogs_test_guard().lock().await;
    RATE_LIMITER.reset();
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
    let signals = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let recorded = signals.clone();
    let observer: DiscogsValidationObserver = Arc::new(move |sig| {
        recorded.lock().unwrap().push(match sig {
            DiscogsKeySignal::Rejected => "rejected",
            DiscogsKeySignal::Accepted => "accepted",
        });
    });
    let mut client = DiscogsClient::with_observer("token".to_string(), observer);
    client.base_url = url;

    client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect("retry should return the successful search response");

    assert_eq!(request_count.load(Ordering::SeqCst), 2);
    assert_eq!(*signals.lock().unwrap(), vec!["accepted"]);
}
