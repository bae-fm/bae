use super::*;

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

/// Spawn a localhost HTTP server that returns the given responses in order,
/// clamping to the last once exhausted.
async fn start_mock(responses: Vec<(u16, Vec<u8>)>) -> String {
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct Mock {
        responses: Arc<Vec<(u16, Vec<u8>)>>,
        index: Arc<AtomicUsize>,
    }

    async fn handler(State(mock): State<Mock>) -> (StatusCode, Vec<u8>) {
        let index = mock
            .index
            .fetch_add(1, Ordering::SeqCst)
            .min(mock.responses.len() - 1);
        let (status, body) = &mock.responses[index];
        (StatusCode::from_u16(*status).unwrap(), body.clone())
    }

    let state = Mock {
        responses: Arc::new(responses),
        index: Arc::new(AtomicUsize::new(0)),
    };
    let app = axum::Router::new().fallback(handler).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}/cover.jpg")
}

#[derive(Clone)]
struct CountingHost {
    status: u16,
    body: Arc<Vec<u8>>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

impl CountingHost {
    fn hits(&self) -> usize {
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }
}

async fn start_counting_host(status: u16, body: Vec<u8>) -> (CountingHost, String) {
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn handler(State(host): State<CountingHost>) -> (StatusCode, Vec<u8>) {
        host.hits.fetch_add(1, Ordering::SeqCst);
        (
            StatusCode::from_u16(host.status).expect("valid test status"),
            host.body.as_ref().clone(),
        )
    }

    let host = CountingHost {
        status,
        body: Arc::new(body),
        hits: Arc::new(AtomicUsize::new(0)),
    };
    let app = axum::Router::new()
        .fallback(handler)
        .with_state(host.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (host, format!("http://{address}/cover.jpg"))
}

async fn start_declared_length_response(content_length: usize) -> String {
    use tokio::io::AsyncWriteExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
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
    format!("http://{address}/cover.jpg")
}

#[tokio::test]
async fn a_second_read_uses_the_stored_image() {
    let body = vec![0xABu8; 256];
    let (host, url) = start_counting_host(200, body.clone()).await;
    let cache = RemoteImageCache::for_test();

    let first = cache.fetch_required(&url).await.unwrap();
    let second = cache.fetch_required(&url).await.unwrap();

    assert_eq!(first.bytes, body);
    assert_eq!(second.bytes, body);
    assert_eq!(host.hits(), 1);
}

#[tokio::test]
async fn a_downloaded_image_survives_a_new_cache_over_the_same_directory() {
    let body = vec![0x3Cu8; 512];
    let (host, url) = start_counting_host(200, body.clone()).await;
    let directory = tempfile::TempDir::new().expect("a temp image-cache directory");

    let first_cache = RemoteImageCache::in_dir(
        directory.path().to_path_buf(),
        u64::MAX,
        Duration::from_millis(1),
    );
    assert_eq!(first_cache.fetch_required(&url).await.unwrap().bytes, body);
    drop(first_cache);

    let second_cache = RemoteImageCache::in_dir(
        directory.path().to_path_buf(),
        u64::MAX,
        Duration::from_millis(1),
    );
    assert_eq!(second_cache.fetch_required(&url).await.unwrap().bytes, body);
    assert_eq!(host.hits(), 1, "the second cache reads the stored image");
}

#[test]
fn disk_cache_evicts_the_oldest_entry_when_over_budget() {
    let directory = tempfile::TempDir::new().expect("a temp image-cache directory");
    let unlimited = DiskImageCache::new(directory.path().to_path_buf(), u64::MAX);
    unlimited.put(
        "https://host/a",
        &DiskImageEntry::Image(RemoteImage {
            bytes: vec![0x11; 400],
            content_type: ContentType::Jpeg,
        }),
    );
    let entry_size = std::fs::read_dir(directory.path())
        .expect("cache directory should read")
        .next()
        .expect("cache entry should exist")
        .expect("cache entry should read")
        .metadata()
        .expect("cache entry metadata should read")
        .len();

    let bounded = DiskImageCache::new(directory.path().to_path_buf(), entry_size * 2);
    bounded.put(
        "https://host/b",
        &DiskImageEntry::Image(RemoteImage {
            bytes: vec![0x22; 400],
            content_type: ContentType::Jpeg,
        }),
    );
    bounded.get("https://host/a");
    bounded.put(
        "https://host/c",
        &DiskImageEntry::Image(RemoteImage {
            bytes: vec![0x33; 400],
            content_type: ContentType::Jpeg,
        }),
    );

    assert!(bounded.get("https://host/a").is_some());
    assert!(bounded.get("https://host/b").is_none());
    assert!(bounded.get("https://host/c").is_some());
}

#[tokio::test]
async fn concurrent_reads_of_one_url_share_the_download() {
    let body = vec![0xABu8; 256];
    let (host, url) = start_counting_host(200, body.clone()).await;
    let cache = RemoteImageCache::for_test();

    let (first, second) = tokio::join!(cache.fetch_required(&url), cache.fetch_required(&url));

    assert_eq!(first.unwrap().bytes, body);
    assert_eq!(second.unwrap().bytes, body);
    assert_eq!(host.hits(), 1, "one URL has one in-flight download");
}

#[tokio::test]
async fn download_rejects_too_small_response() {
    let url = start_mock(vec![(200, vec![0u8; 50])]).await;
    let error = RemoteImageCache::for_test().fetch(&url).await.unwrap_err();
    assert!(
        matches!(&error, ImportError::CoverArt { detail } if detail.contains("too small")),
        "got: {error}"
    );
}

#[tokio::test]
async fn download_rejects_declared_over_cap_response() {
    let url = start_declared_length_response(crate::util::http::MAX_IMAGE_BYTES + 1).await;
    let cache = RemoteImageCache::for_test();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        cache.fetch_required(&url),
    )
    .await
    .expect("oversized response should fail before reading the body");
    let error = result.unwrap_err();
    assert!(
        matches!(&error, ImportError::CoverArt { detail } if detail.contains("too large")),
        "got: {error}"
    );
}

#[tokio::test]
async fn download_retries_transient_then_succeeds() {
    let body = vec![0xCDu8; 256];
    let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
    let image = RemoteImageCache::for_test()
        .fetch_required(&url)
        .await
        .unwrap();
    assert_eq!(image.bytes, body);
}

#[tokio::test]
async fn no_image_answer_is_stored() {
    let (host, url) = start_counting_host(404, Vec::new()).await;
    let cache = RemoteImageCache::for_test();

    assert!(cache.fetch(&url).await.unwrap().is_none());
    assert!(cache.fetch(&url).await.unwrap().is_none());
    assert_eq!(host.hits(), 1);
    assert!(cache.fetch_required(&url).await.is_err());
}
