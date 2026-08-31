use super::*;

/// The archive's addresses are derived from the entity id alone: the
/// release's own front image, and the album's, at fixed paths with a
/// 250px thumbnail beside each.
#[test]
fn cover_art_archive_addresses_are_derived_from_the_entity_id() {
    let base = archive_base();

    let release = RemoteCover::musicbrainz_release("rel-1");
    assert_eq!(release.url, format!("{base}/release/rel-1/front"));
    assert_eq!(
        release.thumbnail_url,
        format!("{base}/release/rel-1/front-250")
    );
    assert_eq!(release.label, "Cover Art Archive");
    assert_eq!(release.source, MetadataSource::MusicBrainz);

    let group = RemoteCover::musicbrainz_release_group("rg-1");
    assert_eq!(group.url, format!("{base}/release-group/rg-1/front"));
    assert_eq!(
        group.thumbnail_url,
        format!("{base}/release-group/rg-1/front-250")
    );
    assert_eq!(group.label, "Cover Art Archive (Album)");
}

#[test]
fn push_unique_cover_dedupes_by_url() {
    let mut covers = vec![RemoteCover {
        url: "https://caa.example/cover.jpg".to_string(),
        thumbnail_url: "https://caa.example/thumb-a.jpg".to_string(),
        label: "Cover Art Archive".to_string(),
        source: MetadataSource::MusicBrainz,
    }];

    push_unique_cover(
        &mut covers,
        RemoteCover {
            url: "https://caa.example/cover.jpg".to_string(),
            thumbnail_url: "https://caa.example/thumb-b.jpg".to_string(),
            label: "Cover Art Archive (Album)".to_string(),
            source: MetadataSource::MusicBrainz,
        },
    );

    assert_eq!(covers.len(), 1);
    assert_eq!(covers[0].thumbnail_url, "https://caa.example/thumb-a.jpg");
}

/// Spawn a localhost HTTP server that returns the given (status, body)
/// responses in order (clamping to the last once exhausted), and return the
/// URL to fetch. Lets the download path exercise real HTTP against
/// controlled responses without reaching the network.
async fn start_mock(responses: Vec<(u16, Vec<u8>)>) -> String {
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct Mock {
        responses: Arc<Vec<(u16, Vec<u8>)>>,
        idx: Arc<AtomicUsize>,
    }

    async fn handler(State(m): State<Mock>) -> (StatusCode, Vec<u8>) {
        let i = m
            .idx
            .fetch_add(1, Ordering::SeqCst)
            .min(m.responses.len() - 1);
        let (code, body) = &m.responses[i];
        (StatusCode::from_u16(*code).unwrap(), body.clone())
    }

    let state = Mock {
        responses: Arc::new(responses),
        idx: Arc::new(AtomicUsize::new(0)),
    };
    let app = axum::Router::new().fallback(handler).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}/cover.jpg")
}

async fn start_declared_length_response(content_length: usize) -> String {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let addr = listener
        .local_addr()
        .expect("test listener should have an address");
    tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("test request should connect");
        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
        stream
            .write_all(response.as_bytes())
            .await
            .expect("test response headers should write");
        std::future::pending::<()>().await;
    });
    format!("http://{addr}/cover.jpg")
}

/// A clock the test moves by hand, so each fetch sees exactly the instant
/// the test chooses — coven's `SteppingClock` advances per `now()` call,
/// which would tie the test to how often the cache reads the clock.
struct TestClock(Mutex<DateTime<Utc>>);

impl TestClock {
    fn at(seconds: i64) -> Arc<Self> {
        Arc::new(Self(Mutex::new(
            DateTime::from_timestamp(seconds, 0).expect("a valid test instant"),
        )))
    }

    fn advance(&self, seconds: i64) {
        let mut now = self.0.lock().expect("test clock mutex poisoned");
        *now += chrono::Duration::seconds(seconds);
    }
}

impl coven::Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("test clock mutex poisoned")
    }
}

/// One version of the image an [`ImageHost`] serves.
struct ImageVersion {
    body: Vec<u8>,
    etag: Option<String>,
    cache_control: Option<String>,
}

#[derive(Clone)]
struct ImageHost {
    versions: Arc<Vec<ImageVersion>>,
    current: Arc<std::sync::atomic::AtomicUsize>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
    conditional_hits: Arc<std::sync::atomic::AtomicUsize>,
}

impl ImageHost {
    fn hits(&self) -> usize {
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn conditional_hits(&self) -> usize {
        self.conditional_hits
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn serve_version(&self, index: usize) {
        self.current
            .store(index, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Spawn a localhost image host serving `versions` (the first until the test
/// switches), answering 304 when the request's `If-None-Match` matches the
/// served version's ETag, and counting every request that reached it.
async fn start_image_host(versions: Vec<ImageVersion>) -> (ImageHost, String) {
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, Response, StatusCode};
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn handler(State(host): State<ImageHost>, headers: HeaderMap) -> Response<Body> {
        host.hits.fetch_add(1, Ordering::SeqCst);
        let version = &host.versions[host.current.load(Ordering::SeqCst)];
        let mut builder = Response::builder();
        if let Some(etag) = &version.etag {
            builder = builder.header("etag", etag);
        }
        if let Some(cache_control) = &version.cache_control {
            builder = builder.header("cache-control", cache_control);
        }
        let sent = headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok());
        if sent.is_some() {
            host.conditional_hits.fetch_add(1, Ordering::SeqCst);
        }
        if sent.is_some() && sent == version.etag.as_deref() {
            return builder
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .expect("test 304 response");
        }
        builder
            .status(StatusCode::OK)
            .body(Body::from(version.body.clone()))
            .expect("test 200 response")
    }

    let host = ImageHost {
        versions: Arc::new(versions),
        current: Arc::new(AtomicUsize::new(0)),
        hits: Arc::new(AtomicUsize::new(0)),
        conditional_hits: Arc::new(AtomicUsize::new(0)),
    };
    let app = axum::Router::new()
        .fallback(handler)
        .with_state(host.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (host, format!("http://{addr}/cover.jpg"))
}

/// A cache over `dir` reading `clock`, retrying immediately. The directory
/// is the test's, so each case starts empty and none can serve another's
/// bytes.
fn cache_in(dir: &std::path::Path, clock: Arc<TestClock>) -> RemoteImageCache {
    RemoteImageCache::in_dir(
        clock as ClockRef,
        dir.to_path_buf(),
        Duration::from_millis(1),
    )
}

#[test]
fn max_age_parses_from_cache_control_and_clamps() {
    let with = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(CACHE_CONTROL, value.parse().unwrap());
        max_age_from_cache_control(&headers)
    };
    assert_eq!(with("max-age=600"), Some(Duration::from_secs(600)));
    assert_eq!(
        with("public, max-age=120, immutable"),
        Some(Duration::from_secs(120))
    );
    // Directive names are case-insensitive.
    assert_eq!(with("Max-Age=90"), Some(Duration::from_secs(90)));
    // No max-age directive, and an unparsable one, both mean the response
    // stated no lifetime of its own.
    assert_eq!(with("no-transform"), None);
    assert_eq!(with("max-age=soon"), None);
    assert_eq!(max_age_from_cache_control(&HeaderMap::new()), None);
    // An absurd lifetime still revalidates within the clamp.
    assert_eq!(with("max-age=99999999999"), Some(MAX_REMOTE_IMAGE_TTL));
}

#[tokio::test]
async fn fresh_entry_serves_without_a_request() {
    let body = vec![0xABu8; 256];
    let (host, url) = start_image_host(vec![ImageVersion {
        body: body.clone(),
        etag: Some("\"v1\"".to_string()),
        cache_control: Some("max-age=600".to_string()),
    }])
    .await;
    let clock = TestClock::at(1_700_000_000);
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), clock.clone());

    let first = cache.fetch_required(&url).await.unwrap();
    assert_eq!(first.bytes, body);

    // Still inside the declared 600s lifetime: served from memory.
    clock.advance(599);
    let second = cache.fetch_required(&url).await.unwrap();
    assert_eq!(second.bytes, body);
    assert_eq!(host.hits(), 1, "a fresh entry must not reach the wire");
}

#[tokio::test]
async fn absent_cache_control_falls_back_to_the_default_ttl() {
    let body = vec![0x5Au8; 256];
    let (host, url) = start_image_host(vec![ImageVersion {
        body: body.clone(),
        etag: None,
        cache_control: None,
    }])
    .await;
    let clock = TestClock::at(1_700_000_000);
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), clock.clone());

    cache.fetch_required(&url).await.unwrap();
    clock.advance(DEFAULT_REMOTE_IMAGE_TTL.as_secs() as i64 - 1);
    cache.fetch_required(&url).await.unwrap();
    assert_eq!(host.hits(), 1, "still fresh under the default TTL");

    // Past the default TTL there is no ETag to revalidate with, so the
    // fetch is an unconditional re-download.
    clock.advance(2);
    let refetched = cache.fetch_required(&url).await.unwrap();
    assert_eq!(refetched.bytes, body);
    assert_eq!(host.hits(), 2);
    assert_eq!(host.conditional_hits(), 0);
}

#[tokio::test]
async fn stale_entry_revalidates_and_not_modified_refreshes_the_clock() {
    let body = vec![0xCDu8; 256];
    let (host, url) = start_image_host(vec![ImageVersion {
        body: body.clone(),
        etag: Some("\"v1\"".to_string()),
        cache_control: Some("max-age=10".to_string()),
    }])
    .await;
    let clock = TestClock::at(1_700_000_000);
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), clock.clone());

    cache.fetch_required(&url).await.unwrap();

    // Stale: revalidated with If-None-Match, answered 304, bytes kept.
    clock.advance(11);
    let revalidated = cache.fetch_required(&url).await.unwrap();
    assert_eq!(revalidated.bytes, body);
    assert_eq!(host.hits(), 2);
    assert_eq!(host.conditional_hits(), 1);

    // The 304 restarted the entry's lifetime, so the next read inside it
    // serves from memory instead of revalidating again.
    clock.advance(5);
    cache.fetch_required(&url).await.unwrap();
    assert_eq!(host.hits(), 2, "a 304 must refresh the entry's clock");
}

#[tokio::test]
async fn revalidation_with_new_bytes_replaces_the_entry() {
    let first_body = vec![0x11u8; 256];
    let second_body = vec![0x22u8; 300];
    let (host, url) = start_image_host(vec![
        ImageVersion {
            body: first_body.clone(),
            etag: Some("\"v1\"".to_string()),
            cache_control: Some("max-age=10".to_string()),
        },
        ImageVersion {
            body: second_body.clone(),
            etag: Some("\"v2\"".to_string()),
            cache_control: Some("max-age=10".to_string()),
        },
    ])
    .await;
    let clock = TestClock::at(1_700_000_000);
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), clock.clone());

    cache.fetch_required(&url).await.unwrap();

    host.serve_version(1);
    clock.advance(11);
    let second = cache.fetch_required(&url).await.unwrap();
    assert_eq!(second.bytes, second_body);

    // The replacement is what the next fresh read serves.
    clock.advance(1);
    let third = cache.fetch_required(&url).await.unwrap();
    assert_eq!(third.bytes, second_body);
    assert_eq!(host.hits(), 2);
}

/// The point of keeping bytes on disk: the next launch draws the cover it
/// already has instead of asking for it again.
#[tokio::test]
async fn a_cached_image_survives_a_new_cache_over_the_same_directory() {
    let body = vec![0x3Cu8; 512];
    let (host, url) = start_image_host(vec![ImageVersion {
        body: body.clone(),
        etag: Some("\"v1\"".to_string()),
        cache_control: Some("max-age=600".to_string()),
    }])
    .await;
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let clock = TestClock::at(1_700_000_000);

    let first = cache_in(dir.path(), clock.clone())
        .fetch_required(&url)
        .await
        .unwrap();
    assert_eq!(first.bytes, body);
    assert_eq!(host.hits(), 1);

    // A second cache over the same directory is the next launch.
    let relaunched = cache_in(dir.path(), clock.clone())
        .fetch_required(&url)
        .await
        .unwrap();
    assert_eq!(relaunched.bytes, body);
    assert_eq!(
        host.hits(),
        1,
        "a relaunch must serve what is already on disk rather than re-download it"
    );
}

fn cached(bytes: Vec<u8>) -> CachedImage {
    CachedImage::store(
        bytes,
        ContentType::Jpeg,
        DateTime::from_timestamp(1_700_000_000, 0).expect("a valid test instant"),
        Freshness {
            max_age: Some(Duration::from_secs(600)),
            etag: None,
            last_modified: None,
        },
    )
}

/// Past its budget the cache drops what has gone longest unread, and keeps
/// what is in use. Reading an entry is what makes it recent, so the one
/// re-read after being written outlives the one written after it.
#[test]
fn the_cache_drops_its_least_recently_read_entries_past_its_budget() {
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let entry = cached(vec![0x11u8; 400]);
    let store = DiskImageCache::new(dir.path().to_path_buf(), u64::MAX);
    store.put("https://host/a", &entry);
    let held = std::fs::read_dir(dir.path())
        .expect("the cache directory reads")
        .map(|e| e.expect("an entry").metadata().expect("metadata").len())
        .sum::<u64>();

    // Room for two of these and no more.
    let store = DiskImageCache::new(dir.path().to_path_buf(), held * 2 + held / 2);
    store.put("https://host/b", &cached(vec![0x22u8; 400]));
    // Reading "a" makes "b" the one that has gone longest unread.
    assert!(store.get("https://host/a").is_some());
    let logs = crate::test_logs::capture_logs_at(tracing::Level::DEBUG, || {
        store.put("https://host/c", &cached(vec![0x33u8; 400]));
    });
    assert!(
        logs.contains("dropped 1,"),
        "an eviction has to be visible in the log, got: {logs}"
    );

    assert!(
        store.get("https://host/b").is_none(),
        "the least recently read entry goes when the budget is exceeded"
    );
    assert_eq!(
        store.get("https://host/a").map(|e| e.bytes),
        Some(vec![0x11u8; 400]),
        "the entry that was read again stays"
    );
    assert_eq!(
        store.get("https://host/c").map(|e| e.bytes),
        Some(vec![0x33u8; 400]),
        "so does the one that was just written"
    );
}

/// A cached entry reads back with everything a conditional GET needs, so a
/// restart revalidates rather than re-downloading.
#[test]
fn a_cached_entry_reads_back_with_its_freshness_terms() {
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let store = DiskImageCache::new(dir.path().to_path_buf(), u64::MAX);
    let written = CachedImage::store(
        vec![0x44u8; 300],
        ContentType::Png,
        DateTime::from_timestamp(1_700_000_000, 0).expect("a valid test instant"),
        Freshness {
            max_age: Some(Duration::from_secs(90)),
            etag: Some("\"v7\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
        },
    );
    store.put("https://host/image", &written);

    let read = store
        .get("https://host/image")
        .expect("the entry reads back");
    assert_eq!(read.bytes, written.bytes);
    assert_eq!(read.content_type, ContentType::Png);
    assert_eq!(read.fetched_at, written.fetched_at);
    assert_eq!(read.freshness.max_age, Some(Duration::from_secs(90)));
    assert_eq!(read.freshness.etag.as_deref(), Some("\"v7\""));
    assert_eq!(
        read.freshness.last_modified.as_deref(),
        Some("Wed, 21 Oct 2015 07:28:00 GMT")
    );
    assert!(store.get("https://host/never-written").is_none());
}

#[tokio::test]
async fn download_succeeds_and_then_serves_from_cache() {
    let body = vec![0xABu8; 256];
    let other = vec![0x11u8; 256];
    let url = start_mock(vec![(200, body.clone()), (200, other)]).await;
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), TestClock::at(1_700_000_000));

    let first = cache.fetch_required(&url).await.unwrap();
    assert_eq!(first.bytes, body);
    // The second call is served from the session cache: it returns the
    // first body, not the mock's second (different) response.
    let second = cache.fetch_required(&url).await.unwrap();
    assert_eq!(second.bytes, body, "second call should be a cache hit");
}

#[tokio::test]
async fn download_rejects_too_small_response() {
    let url = start_mock(vec![(200, vec![0u8; 50])]).await; // under the 100-byte floor
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), TestClock::at(1_700_000_000));
    let err = cache.fetch(&url).await.unwrap_err();
    assert!(
        matches!(&err, ImportError::CoverArt { detail } if detail.contains("too small")),
        "got: {err}"
    );
}

#[tokio::test]
async fn download_rejects_declared_over_cap_response() {
    let url = start_declared_length_response(crate::util::http::MAX_IMAGE_BYTES + 1).await;
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), TestClock::at(1_700_000_000));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        cache.fetch_required(&url),
    )
    .await
    .expect("oversized response should fail before reading the body");
    let err = result.unwrap_err();
    assert!(
        matches!(&err, ImportError::CoverArt { detail } if detail.contains("too large")),
        "got: {err}"
    );
}

#[tokio::test]
async fn download_retries_transient_then_succeeds() {
    let body = vec![0xCDu8; 256];
    // A 503 is transient: retried after backoff, then the 200 succeeds.
    // `for_test` gives the cache a near-zero backoff, so the retry does not
    // sleep the real second.
    let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), TestClock::at(1_700_000_000));
    let image = cache.fetch_required(&url).await.unwrap();
    assert_eq!(image.bytes, body);
}

#[tokio::test]
async fn a_404_is_no_image_rather_than_a_failed_download() {
    // 404 is non-transient: answered without burning the retry budget.
    let url = start_mock(vec![(404, vec![])]).await;
    let dir = tempfile::TempDir::new().expect("a temp image-cache dir");
    let cache = cache_in(dir.path(), TestClock::at(1_700_000_000));
    // A 404 is the host answering that it serves no image at this address —
    // an answer, and the one a derived cover address gets when the archive
    // holds nothing. A caller that needs the bytes names that a failure.
    assert!(cache.fetch(&url).await.unwrap().is_none());
    assert!(cache.fetch_required(&url).await.is_err());
}
