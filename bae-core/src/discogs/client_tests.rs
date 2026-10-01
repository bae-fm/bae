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
        assert!(release.labels.is_empty());
        assert!(pressing.labels.is_empty());
        assert!(release.barcode.is_none());
    }
}

/// Discogs writes a label with no catalog number as `none`, and its data
/// holds other spellings too: each keeps the label's name and no number.
#[test]
fn a_none_catalog_number_is_no_number() {
    for placeholder in ["none", "None", "- none", "-none-"] {
        let raw = serde_json::json!({
            "id": 123, "title": "Album",
            "labels": [{"name": "Label A", "catno": placeholder}, {"catno": placeholder}]
        })
        .to_string();
        assert_eq!(
            parse_discogs_release_json(&raw).unwrap().labels,
            vec![crate::pressing::ReleaseLabel::of(Some("Label A"), None)],
            "{placeholder}"
        );
    }
}

/// Every label is read with its own catalog number, and the pressing keeps
/// them all.
#[test]
fn every_label_and_its_catalog_number_is_read() {
    use crate::pressing::ReleaseLabel;
    let raw = serde_json::json!({
        "id": 123, "title": "Album",
        "labels": [
            {"name": "Label A", "catno": "AB 100"},
            {"name": "Label B", "catno": "CL 719"},
            {"name": "Label B", "catno": "CL 719"},
            {"name": "Label C", "catno": ""}
        ]
    })
    .to_string();
    let release = parse_discogs_release_json(&raw).unwrap();
    let expected = vec![
        ReleaseLabel::of(Some("Label A"), Some("AB 100")),
        ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ReleaseLabel::of(Some("Label C"), None),
    ];
    assert_eq!(release.labels, expected);
    assert_eq!(
        crate::import::discogs_mapper::pressing(&release).0.labels,
        expected
    );
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
    // A server error says nothing about the key, so it must not signal — a
    // transient blip cannot be allowed to reject a good key.
    client.observe::<()>(&Err(DiscogsError::Provider {
        status: StatusCode::SERVICE_UNAVAILABLE,
        told_wait: None,
    }));

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

/// A 429 is Discogs's rate, not its answer: the request is sent again once
/// the window has room, however many times that takes — more than the retry
/// policy would ever repeat a failure — and the lookup answers rather than
/// failing.
#[tokio::test]
async fn a_rate_limit_is_sent_again_until_it_answers_never_failed() {
    let refusals = RETRY.attempts() as usize + 2;
    let mut script = vec![RATE_LIMITED; refusals];
    script.push(SEARCH_OK_EMPTY);
    let (url, request_count) = discogs_response_server(script).await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

    let releases = client
        .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
        .await
        .expect("a rate limit is waited out, not failed");

    assert!(releases.is_empty());
    assert_eq!(request_count.load(Ordering::SeqCst), refusals + 1);
}

/// A 429 holds the whole limiter for the wait it states, so every other
/// Discogs request waits with the refused one.
#[tokio::test]
async fn a_rate_limit_holds_every_discogs_request_for_its_stated_wait() {
    const RATE_LIMITED_HALF_A_MINUTE: &str =
        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\n\r\n";
    let (url, request_count) = discogs_response_server(vec![RATE_LIMITED_HALF_A_MINUTE]).await;
    let discogs = served_by(&url);
    let client = DiscogsClient::new(discogs.clone(), "token".to_string());
    let asked = tokio::spawn(async move {
        client
            .search_with_params(&DiscogsSearchParams::default(), CallPriority::Interactive)
            .await
    });

    let opens_at = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(at) = discogs.limiter.opens_at() {
                return at;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the refusal reaches the limiter");
    asked.abort();

    assert!(opens_at >= tokio::time::Instant::now() + Duration::from_secs(25));
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
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
        matches!(
            error,
            DiscogsError::Provider {
                status: StatusCode::BAD_REQUEST,
                ..
            }
        ),
        "expected Provider(400), got {error:?}",
    );
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
}

#[test]
fn retry_policy_repeats_only_transient_failures() {
    let provider = |status: StatusCode| {
        repeat_discogs(&DiscogsError::Provider {
            status,
            told_wait: None,
        })
    };
    assert_eq!(
        repeat_discogs(&DiscogsError::Provider {
            status: StatusCode::SERVICE_UNAVAILABLE,
            told_wait: Some(Duration::from_secs(9))
        }),
        Repeat::AfterToldWait(Duration::from_secs(9)),
        "a stated wait replaces the backoff"
    );
    assert_eq!(
        provider(StatusCode::INTERNAL_SERVER_ERROR),
        Repeat::AfterBackoff
    );
    assert_eq!(
        provider(StatusCode::SERVICE_UNAVAILABLE),
        Repeat::AfterBackoff
    );
    assert_eq!(provider(StatusCode::BAD_REQUEST), Repeat::Never);
    assert_eq!(provider(StatusCode::FORBIDDEN), Repeat::Never);
    assert_eq!(provider(StatusCode::UNPROCESSABLE_ENTITY), Repeat::Never);
    assert_eq!(repeat_discogs(&DiscogsError::InvalidApiKey), Repeat::Never);
    assert_eq!(repeat_discogs(&DiscogsError::NotFound), Repeat::Never);
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

/// A server error is the provider's momentary state, not its answer: every
/// retry goes to the wire, and so does the next call.
#[tokio::test]
async fn transient_failures_are_not_kept() {
    let attempts = RETRY.attempts() as usize;
    let mut script: Vec<(u16, String)> = (0..attempts).map(|_| (503, String::new())).collect();
    script.push((200, release_body(510003)));
    let (url, requests) = scripted_server(script).await;
    let client = client_at(url);

    let error = client
        .get_release("510003", CallPriority::Interactive)
        .await
        .expect_err("transient answers on every try exhaust the retries");
    assert!(matches!(
        error,
        DiscogsError::Provider {
            status: StatusCode::SERVICE_UNAVAILABLE,
            ..
        }
    ));
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

/// A 200 response carrying `body`, leaked into the `'static` the test server
/// hands out.
fn ok_json(body: String) -> &'static str {
    Box::leak(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_boxed_str(),
    )
}

/// One page of catalog-number results, of `pages` in all.
fn catalog_page(pages: u32, releases: &[(u64, &str)]) -> &'static str {
    let results: Vec<serde_json::Value> = releases
        .iter()
        .map(|(id, catno)| {
            serde_json::json!({
                "id": id, "type": "release", "title": "Artist One - Album One",
                "catno": catno, "year": "1955"
            })
        })
        .collect();
    ok_json(
        serde_json::json!({
            "pagination": { "page": 1, "pages": pages, "per_page": 100, "items": results.len() },
            "results": results
        })
        .to_string(),
    )
}

/// Discogs orders a catalog-number search by relevance, so a release under
/// exactly the number asked can sit past the first page: every page is read.
#[tokio::test]
async fn a_catalog_number_search_reads_every_page() {
    let (url, request_count) = discogs_response_server(vec![
        catalog_page(2, &[(1, "LBL 1719"), (2, "XLBL 719")]),
        catalog_page(2, &[(3, "LBL 719")]),
    ])
    .await;
    let client = DiscogsClient::new(served_by(&url), "token".to_string());

    let releases = client
        .search_with_params(
            &DiscogsSearchParams {
                catno: Some("LBL 719".to_string()),
                ..Default::default()
            },
            CallPriority::Interactive,
        )
        .await
        .expect("both pages answer");

    assert_eq!(
        releases
            .iter()
            .map(|release| release.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(request_count.load(Ordering::SeqCst), 2);
}

// ── Discogs's window under sustained load ───────────────────────────────────

/// Discogs's count of one address as it describes it: every request that
/// arrived in the last sixty seconds, sixty at most, one over that turned
/// away with a 429. Its headers count the window as the request found it —
/// a fresh window answers sixty remaining.
#[derive(Default)]
struct DiscogsMinute {
    arrivals: std::collections::VecDeque<tokio::time::Instant>,
}

impl crate::util::rate_limiter::load_model::Window for DiscogsMinute {
    fn arrive(&mut self, at: tokio::time::Instant) -> (StatusCode, reqwest::header::HeaderMap) {
        while self
            .arrivals
            .front()
            .is_some_and(|arrived| *arrived + DISCOGS_WINDOW <= at)
        {
            self.arrivals.pop_front();
        }
        let used = self.arrivals.len();
        let status = if used < 60 {
            self.arrivals.push_back(at);
            StatusCode::OK
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-discogs-ratelimit", 60.into());
        headers.insert("x-discogs-ratelimit-used", used.into());
        headers.insert("x-discogs-ratelimit-remaining", (60 - used).into());
        (status, headers)
    }
}

fn discogs_load(others: Vec<Duration>) -> crate::util::rate_limiter::load_model::Load {
    crate::util::rate_limiter::load_model::Load {
        requests: 300,
        latency: Duration::from_millis(50)..Duration::from_secs(1),
        others,
        seed: 27,
    }
}

/// Requests spaced as Discogs's limiter spaces them, reaching Discogs after
/// trips of different lengths, never put a sixty-first in any minute — and the
/// window's own count does not slow a client that has it to itself.
#[tokio::test(start_paused = true)]
async fn sustained_load_stays_inside_discogs_minute() {
    let discogs = Discogs::new(Http::for_test());
    let limiter = Arc::new(discogs.limiter);
    let load = discogs_load(Vec::new());
    let requests = load.requests as u32;

    let outcome = crate::util::rate_limiter::load_model::sustain(
        limiter,
        DiscogsMinute::default(),
        discogs_rate_answer,
        load,
    )
    .await;

    assert_eq!(outcome.refused, 0, "Discogs turned requests away");
    assert!(
        outcome.elapsed <= DISCOGS_REQUEST_INTERVAL * requests + Duration::from_secs(2),
        "took {:?}: the count slowed a client alone on its address",
        outcome.elapsed
    );
}

/// Another client on the same address spending thirty requests a minute
/// leaves less of the window; the count each response carries slows this
/// client before the window is spent, rather than after a 429 — and only that
/// far: as its own requests leave the window it takes their places again, so
/// it keeps at least the thirty a minute left over, less the reserve.
#[tokio::test(start_paused = true)]
async fn sustained_load_yields_discogs_minute_to_another_client() {
    let discogs = Discogs::new(Http::for_test());
    let limiter = Arc::new(discogs.limiter);
    let others = (0..300)
        .map(|second| Duration::from_secs(second * 2))
        .collect();
    let load = discogs_load(others);
    let requests = load.requests as u32;

    let outcome = crate::util::rate_limiter::load_model::sustain(
        limiter,
        DiscogsMinute::default(),
        discogs_rate_answer,
        load,
    )
    .await;

    assert_eq!(outcome.refused, 0, "Discogs turned requests away");
    let left_over_per_minute = 60 - 30 - 3;
    assert!(
        outcome.elapsed <= DISCOGS_WINDOW * requests / left_over_per_minute,
        "took {:?}: slowed past the share another client leaves",
        outcome.elapsed
    );
}
