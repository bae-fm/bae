use super::*;
use crate::import::cover_art::{DownscaledCopy, RemoteImageSet};
use crate::import::Catalog;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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

/// A Discogs transport whose requests go to the local server at `origin`.
fn served_by(origin: &str) -> Arc<Discogs> {
    Arc::new(Discogs::for_test(
        Http::for_test().serve("api.discogs.com", origin),
    ))
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
        formats: Vec::new(),
        country: None,
        label: None,
        catno: None,
        barcode: Vec::new(),
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

    assert_eq!(
        cover.image,
        RemoteImageSet::original("https://discogs.example/thumb.jpg".to_string())
    );
    assert_eq!(cover.label, Catalog::Discogs.cover_source_label());
    assert_eq!(cover.source, Catalog::Discogs);
}

#[test]
fn search_result_remote_cover_reads_the_cover_everywhere_when_thumb_is_absent() {
    let result = search_result_with_cover_fields(Some("https://discogs.example/full.jpg"), None);

    let cover = result.remote_cover().unwrap();

    assert_eq!(
        cover.image,
        RemoteImageSet::original("https://discogs.example/full.jpg".to_string())
    );
    assert_eq!(cover.label, Catalog::Discogs.cover_source_label());
    assert_eq!(cover.source, Catalog::Discogs);
}

#[test]
fn search_result_thumb_is_the_cover_bounded_to_150_pixels() {
    let result = search_result_with_cover_fields(
        Some("https://discogs.example/full.jpg"),
        Some("https://discogs.example/thumb.jpg"),
    );

    let cover = result.remote_cover().unwrap();

    assert_eq!(
        cover.image,
        RemoteImageSet {
            url: "https://discogs.example/full.jpg".to_string(),
            downscaled: vec![DownscaledCopy {
                url: "https://discogs.example/thumb.jpg".to_string(),
                max_edge: 150,
            }],
        }
    );
    // A 40-point row reads the thumbnail; a 200-point pane never does.
    assert_eq!(
        cover.image.url_covering(Some(80)),
        "https://discogs.example/thumb.jpg"
    );
    assert_eq!(
        cover.image.url_covering(Some(400)),
        "https://discogs.example/full.jpg"
    );
}

#[test]
fn search_result_remote_cover_is_absent_without_cover_fields() {
    let result = search_result_with_cover_fields(None, None);

    assert!(result.remote_cover().is_none());
}

#[test]
fn release_images_keep_order_and_normalize_missing_urls() {
    let release = parse_discogs_release_json(
        &serde_json::json!({
            "id": 123, "title": "Album Title", "images": [
                { "type": "secondary", "uri": "https://images.example/back.jpg", "uri150": "" },
                { "type": "primary", "uri": "", "uri150": "https://images.example/front.jpg" },
                { "type": "secondary", "uri": "", "uri150": "" },
                { "type": "secondary", "uri": "https://images.example/back.jpg" }
            ]
        })
        .to_string(),
    )
    .expect("release parses");
    assert_eq!(release.covers.len(), 2);
    assert_eq!(
        release.covers[0].image,
        RemoteImageSet::original("https://images.example/front.jpg".to_string())
    );
    assert_eq!(
        release.covers[1].image,
        RemoteImageSet::original("https://images.example/back.jpg".to_string())
    );
    assert!(release.covers[0].label.contains("[r123]"));
}

#[test]
fn master_images_keep_secondary_artwork() {
    let covers = parse_discogs_master_covers(
        &serde_json::json!({
            "id": 456, "images": [
                { "type": "primary", "uri": "https://images.example/front.jpg" },
                { "type": "secondary", "uri": "https://images.example/booklet.jpg" }
            ]
        })
        .to_string(),
    )
    .expect("master images parse");
    assert_eq!(covers.len(), 2);
    assert!(covers[1].label.contains("[m456]"));
    assert_eq!(covers[1].image.url, "https://images.example/booklet.jpg");
}

#[test]
fn release_without_images_offers_none() {
    let release =
        parse_discogs_release_json(r#"{"id":123,"title":"Album Title"}"#).expect("release parses");
    assert!(release.covers.is_empty());
}

#[test]
fn release_year_distinguishes_unknown_from_known() {
    for (field, expected) in [
        (None, None),
        (Some(serde_json::Value::Null), None),
        (Some(serde_json::json!(0)), None),
        (Some(serde_json::json!(1971)), Some(1971)),
    ] {
        let mut document = serde_json::json!({ "id": 123, "title": "Album Title" });
        if let Some(value) = field {
            document["year"] = value;
        }
        let release = parse_discogs_release_json(&document.to_string()).unwrap();
        assert_eq!(release.year, expected, "{document}");
    }
}

#[test]
fn release_without_master_has_no_album_identity() {
    for (field, expected) in [
        (None, None),
        (Some(serde_json::Value::Null), None),
        (Some(serde_json::json!(0)), None),
        (Some(serde_json::json!(456)), Some("456")),
    ] {
        let mut document =
            serde_json::json!({ "id": 123, "title": "Album Title", "type": "release" });
        if let Some(value) = field {
            document["master_id"] = value;
        }
        let release = parse_discogs_release_json(&document.to_string()).unwrap();
        assert_eq!(release.master_id.as_deref(), expected, "{document}");
        let search: DiscogsSearchResult = serde_json::from_value(document.clone()).unwrap();
        assert_eq!(
            search.master_id.map(|id| id.to_string()).as_deref(),
            expected,
            "search: {document}"
        );
    }
}

#[test]
fn release_barcode_reaches_pressing_metadata() {
    let json = r#"{"id":123,"title":"Album Title","identifiers":[
            {"type":"Matrix / Runout","value":"MATRIX-7"},
            {"type":"Barcode","value":" \t"},
            {"type":"Barcode","value":"0 12345 67890 5","description":"Text"},
            {"type":"Barcode","value":"012345678905","description":"Scanned"}
        ]}"#;
    let release = parse_discogs_release_json(json).unwrap();
    assert_eq!(release.barcode.as_deref(), Some("0 12345 67890 5"));
    let payloads: crate::import::payloads::ReleasePayloads =
        crate::import::payloads::ReleasePayloads::for_test(
            crate::import::MetadataRef::new(crate::import::Catalog::Discogs, "123"),
            json.to_string(),
            Vec::new(),
        );
    assert_eq!(
        payloads
            .extract()
            .unwrap()
            .detail_for_audio(&[], &[])
            .unwrap()
            .barcode,
        release.barcode
    );
    assert_eq!(
        crate::import::discogs_mapper::metadata(&release)
            .pressing
            .barcode
            .as_deref(),
        Some("0 12345 67890 5")
    );
}

#[test]
fn blank_pressing_fields_are_absent_in_discogs_documents() {
    for value in ["", " \t"] {
        let raw = serde_json::json!({
            "id":123, "title":"Album Title", "country":value,
            "formats":[{"name":value}], "labels":[{"name":value,"catno":value}],
            "identifiers":[{"type":"Barcode","value":value}]
        })
        .to_string();
        let release = parse_discogs_release_json(&raw).unwrap();
        assert!(release.country.is_none());
        let (pressing, media) = crate::import::discogs_mapper::pressing(&release);
        assert!(pressing.facts.is_empty());
        assert_eq!(media, crate::pressing::StatedMedia::Undescribed);
        assert!(release.label.is_empty());
        assert!(release.catno.is_none());
        assert!(release.barcode.is_none());
    }
}

#[test]
fn master_year_distinguishes_unknown_from_known() {
    for (document, expected) in [
        (r#"{"id":456}"#, None),
        (r#"{"id":456,"year":null}"#, None),
        (r#"{"id":456,"year":0}"#, None),
        (r#"{"id":456,"year":1966}"#, Some(1966)),
    ] {
        assert_eq!(
            parse_discogs_master_json(document).unwrap().year,
            expected,
            "{document}"
        );
    }
}

#[test]
fn master_parser_retains_album_metadata_without_pressing_or_track_defaults() {
    let master = parse_discogs_master_json(
        r#"{
        "id":456, "title":"Album Title", "year":1982,
        "artists":[{"id":12,"name":"Artist Name"}],
        "main_release":123,
        "images":[{"type":"primary","uri":"https://images.example/front.jpg"}]
    }"#,
    )
    .unwrap();
    assert_eq!(master.title.as_deref(), Some("Album Title"));
    assert_eq!(master.year, Some(1982));
    assert_eq!(master.artists[0].id, "12");
    assert_eq!(master.artists[0].name, "Artist Name");
    assert_eq!(master.covers.len(), 1);
    assert_eq!(
        master.covers[0].image.url,
        "https://images.example/front.jpg"
    );

    for raw in [r#"{"id":456}"#, r#"{"id":456,"title":"","year":0}"#] {
        let master = parse_discogs_master_json(raw).unwrap();
        assert!(master.title.is_none());
        assert!(master.year.is_none());
        assert!(master.artists.is_empty());
        assert!(master.covers.is_empty());
    }
    assert!(parse_discogs_master_json(r#"{"id":456,"year":"broken"}"#).is_err());
}

#[tokio::test]
async fn master_fetch_returns_its_own_document_without_following_main_release() {
    let raw =
        r#"{"id":510005,"title":"Album Title","year":1982,"main_release":510006}"#.to_string();
    let (url, requests) = scripted_server(vec![(200, raw.clone())]).await;
    let (master, archived) = client_at(url)
        .get_master("510005", CallPriority::Interactive)
        .await
        .unwrap();
    assert_eq!(master.title.as_deref(), Some("Album Title"));
    assert_eq!(master.year, Some(1982));
    assert_eq!(archived, raw);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[test]
fn release_parser_preserves_nested_tracklist_entries() {
    let release = parse_discogs_release_json(
        &serde_json::json!({
            "id": 123,
            "title": "Album Title",
            "tracklist": [{
                "position": "",
                "type_": "index",
                "title": "Suite Title",
                "sub_tracks": [
                    { "position": "1a", "type_": "track", "title": "Movement One" },
                    { "position": "1b", "type_": "track", "title": "Movement Two" }
                ]
            }]
        })
        .to_string(),
    )
    .expect("nested tracklist parses");

    assert_eq!(release.tracklist.len(), 1);
    assert_eq!(release.tracklist[0].sub_tracks.len(), 2);
    assert_eq!(release.tracklist[0].sub_tracks[1].position, "1b");
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
    let client = DiscogsClient::with_observer(
        Arc::new(Discogs::for_test(Http::for_test())),
        "token".to_string(),
        observer,
    );

    client.observe::<()>(&Ok(()));
    client.observe::<()>(&Err(DiscogsError::InvalidApiKey));
    // A rate limit says nothing about the key, so it must not signal — a
    // transient blip cannot be allowed to reject a good key.
    client.observe::<()>(&Err(DiscogsError::RateLimit));

    assert_eq!(*signals.lock().unwrap(), vec!["accepted", "rejected"]);
}

#[tokio::test]
async fn transport_error_display_does_not_include_discogs_token() {
    let token = "secret-discogs-token";
    let client = DiscogsClient::new(served_by("http://127.0.0.1:1"), token.to_string());

    let error = client
        .validate_token(CallPriority::Interactive)
        .await
        .unwrap_err();

    assert!(!error.to_string().contains(token));
}

#[tokio::test]
async fn validate_token_sends_token_in_authorization_header() {
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
    let client = DiscogsClient::new(served_by(&url), token.to_string());

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
async fn search_retries_rate_limit_then_returns_success() {
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

    let releases = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect("retry should return the successful search response");

    assert!(releases.is_empty());
    assert_eq!(request_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn search_returns_persistent_rate_limit_after_retry_attempts() {
    let attempts = RETRY.attempts() as usize;
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED; attempts]).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("persistent rate limit should fail after retry attempts");

    assert!(matches!(error, DiscogsError::RateLimit));
    assert_eq!(request_count.load(Ordering::SeqCst), attempts);
}

#[tokio::test]
async fn search_does_not_retry_invalid_api_key() {
    let (url, request_count) = discogs_response_server(vec![UNAUTHORIZED]).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

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
async fn search_does_not_retry_client_error() {
    let (url, request_count) = discogs_response_server(vec![BAD_REQUEST]).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

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
async fn search_does_not_retry_not_found() {
    let (url, request_count) = discogs_response_server(vec![NOT_FOUND]).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

    let error = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect_err("not found should fail without retry");

    assert!(matches!(error, DiscogsError::NotFound));
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn search_observer_records_one_signal_after_internal_retry() {
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED, SEARCH_OK_EMPTY]).await;
    let signals = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let recorded = signals.clone();
    let observer: DiscogsValidationObserver = Arc::new(move |sig| {
        recorded.lock().unwrap().push(match sig {
            DiscogsKeySignal::Rejected => "rejected",
            DiscogsKeySignal::Accepted => "accepted",
        });
    });
    let client = DiscogsClient::with_observer(served_by(&url), "token".to_string(), observer);

    client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect("retry should return the successful search response");

    assert_eq!(request_count.load(Ordering::SeqCst), 2);
    assert_eq!(*signals.lock().unwrap(), vec!["accepted"]);
}

// ── The response cache ──────────────────────────────────────────────────────

/// A local HTTP server answering `responses` in order, then answering anything
/// further with a status no test expects — an over-count fails on the
/// assertion instead of hanging until the request timeout.
///
/// One connection per response: the stream is dropped once the body is written,
/// so the client opens a fresh connection for its next request and the accept
/// count is the request count.
async fn scripted_server(responses: Vec<(u16, String)>) -> (String, Arc<AtomicUsize>) {
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
        let mut remaining = responses.into_iter();
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            counted_requests.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer).await;
            let (status, body) = remaining
                .next()
                .unwrap_or_else(|| (599, "unscripted request".to_string()));
            let raw = format!(
                "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(raw.as_bytes()).await;
        }
    });
    (url, request_count)
}

fn release_body(id: u64) -> String {
    serde_json::json!({ "id": id, "title": "Album Title" }).to_string()
}

fn client_at(url: String) -> DiscogsClient {
    DiscogsClient::new(served_by(&url), "token".to_string())
}

#[tokio::test]
async fn a_repeated_request_is_answered_without_a_second_round_trip() {
    let (url, requests) = scripted_server(vec![(200, release_body(510001))]).await;
    let client = client_at(url);

    let (first, first_raw) = client
        .get_release("510001", CallPriority::Interactive)
        .await
        .expect("the release fetch succeeds");
    let (second, second_raw) = client
        .get_release("510001", CallPriority::Interactive)
        .await
        .expect("the repeated fetch is answered");

    assert_eq!(first.id, second.id);
    assert_eq!(first_raw, second_raw);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_not_found_answer_is_kept() {
    let (url, requests) = scripted_server(vec![(404, String::new())]).await;
    let client = client_at(url);

    for _ in 0..2 {
        let error = client
            .get_release("510002", CallPriority::Interactive)
            .await
            .expect_err("Discogs has no such release");
        assert!(matches!(error, DiscogsError::NotFound));
    }

    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// A rate limit and a server error are the provider's momentary state, not its
/// answer: every retry goes to the wire, and so does the next call.
#[tokio::test]
async fn transient_failures_are_not_kept() {
    let attempts = RETRY.attempts() as usize;
    // Every try but the last is a server error; the last is a rate limit.
    let mut script: Vec<(u16, String)> = (1..attempts).map(|_| (503, String::new())).collect();
    script.push((429, String::new()));
    script.push((200, release_body(510003)));
    let (url, requests) = scripted_server(script).await;
    let client = client_at(url);

    let error = client
        .get_release("510003", CallPriority::Interactive)
        .await
        .expect_err("transient answers on every try exhaust the retries");
    assert!(matches!(error, DiscogsError::RateLimit));
    assert_eq!(
        requests.load(Ordering::SeqCst),
        attempts,
        "each retry asked the server again"
    );

    let (release, _) = client
        .get_release("510003", CallPriority::Interactive)
        .await
        .expect("the provider recovered");
    assert_eq!(release.id, "510003");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        attempts + 1,
        "the failed answer was not kept"
    );
}

/// Each transport keeps its own answers: two asking the same URL each ask
/// their own server.
#[tokio::test]
async fn two_transports_keep_their_own_answers() {
    let (first_url, first_requests) = scripted_server(vec![(200, release_body(510004))]).await;
    let (second_url, second_requests) = scripted_server(vec![(200, release_body(510004))]).await;

    let (_, first_raw) = client_at(first_url)
        .get_release("510004", CallPriority::Interactive)
        .await
        .expect("the first server answers");
    let (_, second_raw) = client_at(second_url)
        .get_release("510004", CallPriority::Interactive)
        .await
        .expect("the second server answers");

    assert_eq!(first_raw, second_raw);
    assert_eq!(first_requests.load(Ordering::SeqCst), 1);
    assert_eq!(second_requests.load(Ordering::SeqCst), 1);
}
