use super::*;
use crate::remote_image::tests::*;

#[tokio::test]
async fn a_second_read_uses_the_stored_image() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let cache = RemoteImageCache::for_test(direct_http());

    let first = cache.fetch_required(&url).await.unwrap();
    let second = cache.fetch_required(&url).await.unwrap();

    assert_eq!(first.bytes, body);
    assert_eq!(second.bytes, body);
    assert_eq!(host.hits(), 1);
}

#[tokio::test]
async fn a_downloaded_image_survives_a_new_cache_over_the_same_directory() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let directory = tempfile::TempDir::new().expect("a temp image-cache directory");

    let first_cache =
        RemoteImageCache::in_dir(direct_http(), directory.path().to_path_buf(), u64::MAX);
    assert_eq!(first_cache.fetch_required(&url).await.unwrap().bytes, body);
    drop(first_cache);

    let second_cache =
        RemoteImageCache::in_dir(direct_http(), directory.path().to_path_buf(), u64::MAX);
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

/// A panic that poisoned the access order leaves the cache reading, writing
/// and evicting: the order starts over instead of failing every later call.
#[test]
fn a_poisoned_access_order_starts_over_and_the_cache_keeps_working() {
    let directory = tempfile::TempDir::new().expect("a temp image-cache directory");
    let cache = DiskImageCache::new(directory.path().to_path_buf(), u64::MAX);
    let image = |byte| {
        DiskImageEntry::Image(RemoteImage {
            bytes: vec![byte; 400],
            content_type: ContentType::Jpeg,
        })
    };
    cache.put("https://host/a", &image(0x11));

    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.with_access(|_| panic!("a change to the access order broke"))
    }));
    assert!(poisoned.is_err());
    assert!(cache.access.is_poisoned());

    assert!(cache.get("https://host/a").is_some());
    cache.put("https://host/b", &image(0x22));
    assert!(cache.get("https://host/b").is_some());
    assert!(!cache.access.is_poisoned(), "the order started over");
}

#[tokio::test]
async fn concurrent_reads_of_one_url_share_the_download() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let cache = RemoteImageCache::for_test(direct_http());

    let (first, second) = tokio::join!(cache.fetch_required(&url), cache.fetch_required(&url));

    assert_eq!(first.unwrap().bytes, body);
    assert_eq!(second.unwrap().bytes, body);
    assert_eq!(host.hits(), 1, "one URL has one in-flight download");
}

#[tokio::test]
async fn no_image_answer_is_stored() {
    let (host, url) = start_counting_host(404, Vec::new()).await;
    let cache = RemoteImageCache::for_test(direct_http());

    assert!(cache.fetch(&url).await.unwrap().is_none());
    assert!(cache.fetch(&url).await.unwrap().is_none());
    assert_eq!(host.hits(), 1);
    assert!(cache.fetch_required(&url).await.is_err());
}

/// An address its host serves no image at, where one is required, is that
/// answer: no image there, not a status the host never sent.
#[tokio::test]
async fn required_missing_artwork_is_no_image() {
    let (host, url) = start_counting_host(404, Vec::new()).await;
    let cache = RemoteImageCache::for_test(direct_http());
    assert!(cache.fetch(&url).await.unwrap().is_none());
    let error = cache.fetch_required(&url).await.unwrap_err();
    assert_eq!(host.hits(), 1);
    assert!(
        matches!(&error, RemoteImageError::NoImage { url: missing } if *missing == url),
        "{error}"
    );
}
