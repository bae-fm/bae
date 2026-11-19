use super::*;
use crate::remote_image::tests::*;

/// A test store of the catalogs' part, which the returned folder holds.
async fn test_store() -> (Store, bae_util::test_files::TestFolder) {
    bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await
}

/// The folder a `for_test` cache keeps its files in.
fn owned_folder(cache: &RemoteImageCache) -> &bae_util::test_files::TestFolder {
    cache._owned_dir.as_deref().expect("a cache made by `for_test` owns its folder")
}

/// How many rows `cached_image` holds, and the sum of their bytes.
async fn held(store: &Store) -> (u64, u64) {
    store
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM cached_image",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap()
}

fn image(byte: u8) -> DiskImageEntry {
    DiskImageEntry::Image(RemoteImage { bytes: vec![byte; 400], content_type: ContentType::Jpeg })
}

#[tokio::test]
async fn a_second_read_uses_the_stored_image() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let (cache, _store) = test_cache().await;

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
    let directory = bae_util::test_files::TestFolder::new();
    let (store, _store_dir) = test_store().await;
    let open = || {
        RemoteImageCache::in_dir(
            direct_http(),
            store.clone(),
            directory.path().to_path_buf(),
            REMOTE_IMAGE_DISK_BUDGET,
            bae_diagnostics::Diagnostics::noop(),
        )
    };

    let first_cache = open();
    assert_eq!(first_cache.fetch_required(&url).await.unwrap().bytes, body);
    drop(first_cache);

    let second_cache = open();
    assert_eq!(second_cache.fetch_required(&url).await.unwrap().bytes, body);
    assert_eq!(host.hits(), 1, "the second cache reads the stored image");
}

/// A write over the budget deletes the least recently used rows, a read
/// counting as a use, and removes their files.
#[tokio::test]
async fn a_write_over_the_budget_evicts_the_least_recently_used_entry() {
    let directory = bae_util::test_files::TestFolder::new();
    let (store, _store_dir) = test_store().await;
    let diagnostics = bae_diagnostics::Diagnostics::noop();
    let unlimited = DiskImageCache::new(
        directory.path().to_path_buf(),
        REMOTE_IMAGE_DISK_BUDGET,
        store.clone(),
        diagnostics.clone(),
    );
    unlimited.put("https://host/a", image(0x11)).await.unwrap();
    let (_, entry_size) = held(&store).await;
    assert_eq!(
        entry_size,
        directory.metadata(file_for("https://host/a")).len(),
        "the row holds the file's size"
    );

    let bounded = DiskImageCache::new(
        directory.path().to_path_buf(),
        entry_size * 2,
        store.clone(),
        diagnostics,
    );
    bounded.put("https://host/b", image(0x22)).await.unwrap();
    assert!(bounded.get("https://host/a").await.unwrap().is_some());
    bounded.put("https://host/c", image(0x33)).await.unwrap();

    assert!(bounded.get("https://host/a").await.unwrap().is_some());
    assert!(bounded.get("https://host/b").await.unwrap().is_none());
    assert!(bounded.get("https://host/c").await.unwrap().is_some());
    assert!(!directory.exists(file_for("https://host/b")), "b's file is removed");
    assert_eq!(held(&store).await, (2, entry_size * 2));
}

#[tokio::test]
async fn concurrent_reads_of_one_url_share_the_download() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let (cache, _store) = test_cache().await;

    let (first, second) = tokio::join!(cache.fetch_required(&url), cache.fetch_required(&url));

    assert_eq!(first.unwrap().bytes, body);
    assert_eq!(second.unwrap().bytes, body);
    assert_eq!(host.hits(), 1, "one URL has one in-flight download");
}

#[tokio::test]
async fn no_image_answer_is_stored() {
    let (host, url) = start_counting_host(404, Vec::new()).await;
    let (cache, _store) = test_cache().await;

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
    let (cache, _store) = test_cache().await;
    assert!(cache.fetch(&url).await.unwrap().is_none());
    let error = cache.fetch_required(&url).await.unwrap_err();
    assert_eq!(host.hits(), 1);
    assert!(
        matches!(&error, RemoteImageError::NoImage { url: missing } if *missing == url),
        "{error}"
    );
}

/// Each write keeps a row with its file's size, and a clear deletes the rows
/// and the files.
#[tokio::test]
async fn the_cache_keeps_a_row_for_each_file_and_a_clear_deletes_both() {
    let (store, _store_dir) = test_store().await;
    let cache = RemoteImageCache::for_test(
        direct_http(),
        store.clone(),
        bae_diagnostics::Diagnostics::noop(),
    );
    assert_eq!(held(&store).await, (0, 0));

    cache.put_for_test("https://host/a", &[0x11; 400]).await;
    let (rows, bytes) = held(&store).await;
    assert_eq!(rows, 1);
    assert!(bytes > 400, "the file and its header: {bytes}");

    cache.clear().await.unwrap();
    assert_eq!(held(&store).await, (0, 0));
    assert!(!owned_folder(&cache).exists(&cache.disk.dir), "the clear removes the folder");

    cache.put_for_test("https://host/b", &[0x22; 100]).await;
    assert_eq!(held(&store).await.0, 1, "the cache takes writes after a clear");
}

#[tokio::test]
async fn clearing_a_cache_that_was_never_written_is_fine() {
    let directory = bae_util::test_files::TestFolder::new();
    let (store, _store_dir) = test_store().await;
    let cache = RemoteImageCache::in_dir(
        direct_http(),
        store.clone(),
        directory.path().join("not-made-yet"),
        REMOTE_IMAGE_DISK_BUDGET,
        bae_diagnostics::Diagnostics::noop(),
    );
    cache.clear().await.unwrap();
    assert_eq!(held(&store).await, (0, 0));
}

/// A file no row names, as a crash between writing a file and its row
/// leaves, is not an answer: the next fetch downloads the image again and
/// records the file.
#[tokio::test]
async fn a_file_no_row_names_is_fetched_again_and_recorded() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let (store, _store_dir) = test_store().await;
    let cache = RemoteImageCache::for_test(
        direct_http(),
        store.clone(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let path = cache.disk.dir.join(file_for(&url));
    owned_folder(&cache).write(&path, b"none\n");

    assert_eq!(cache.fetch_required(&url).await.unwrap().bytes, body);

    assert_eq!(host.hits(), 1);
    assert_eq!(held(&store).await, (1, owned_folder(&cache).metadata(&path).len()));
}

/// A row whose file something other than bae removed is deleted when a read
/// finds the file gone, and the next fetch downloads the image again.
#[tokio::test]
async fn a_row_whose_file_is_gone_is_deleted_and_the_image_fetched_again() {
    let body = test_png();
    let (host, url) = start_counting_host(200, body.clone()).await;
    let (store, _store_dir) = test_store().await;
    let cache = RemoteImageCache::for_test(
        direct_http(),
        store.clone(),
        bae_diagnostics::Diagnostics::noop(),
    );
    cache.fetch_required(&url).await.unwrap();
    owned_folder(&cache).remove(cache.disk.dir.join(file_for(&url)));

    assert!(cache.disk.get(&url).await.unwrap().is_none());
    assert_eq!(held(&store).await, (0, 0));
    assert_eq!(cache.fetch_required(&url).await.unwrap().bytes, body);
    assert_eq!(host.hits(), 2);
}
