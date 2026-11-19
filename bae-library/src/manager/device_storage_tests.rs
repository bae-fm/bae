//! What the device keeps for the library: how the query's sums become the
//! page's parts, and the reads and commands over the open library.

use super::*;
use crate::manager::tests::*;
use bae_records::synced::RELEASE_FILES_NAMESPACE;

/// The sums with nothing pinned, cached or kept from a reset: the audio
/// budget at its default and the library's 700 bytes.
fn summed() -> StorageSums {
    StorageSums {
        audio_budget: Some(CacheLimit::Gb20.bytes()),
        pinned: 0,
        cache: 0,
        library: 700,
        before_reset: None,
        total: 700,
    }
}

/// Pinned and Cache show while they hold bytes, Library always, in that
/// order.
#[test]
fn a_part_with_no_bytes_is_left_out_but_the_library_never() {
    let nothing = DeviceStorage::from_sums(&summed()).unwrap();
    assert_eq!(nothing.parts, [StoragePart { kind: StorageKind::Library, bytes: 700 }]);
    assert_eq!((nothing.total, nothing.cache, nothing.can_clear_cache), (700, 0, false));
    assert_eq!(nothing.before_reset, None);

    let full = DeviceStorage::from_sums(&StorageSums {
        pinned: 4060,
        cache: 235,
        total: 4995,
        ..summed()
    })
    .unwrap();
    assert_eq!(
        full.parts,
        [
            StoragePart { kind: StorageKind::Pinned, bytes: 4060 },
            StoragePart { kind: StorageKind::Cache, bytes: 235 },
            StoragePart { kind: StorageKind::Library, bytes: 700 },
        ]
    );
    assert_eq!((full.total, full.cache, full.can_clear_cache), (4995, 235, true));
    assert_eq!(full.cache_limit, CacheLimit::Gb20);
}

/// From before the reset shows while an archive is here, last.
#[test]
fn the_archives_a_reset_kept_show_last() {
    let sums = StorageSums { before_reset: Some(5000), total: 5700, ..summed() };
    let storage = DeviceStorage::from_sums(&sums).unwrap();
    assert_eq!(
        storage.parts,
        [
            StoragePart { kind: StorageKind::Library, bytes: 700 },
            StoragePart { kind: StorageKind::BeforeReset, bytes: 5000 },
        ]
    );
    assert_eq!((storage.total, storage.before_reset), (5700, Some(5000)));
}

/// A budget no choice names is a failed read, never shown as a default, and
/// so is an audio cache coven has no budget for.
#[test]
fn a_budget_no_choice_names_fails_the_read() {
    let odd = StorageSums { audio_budget: Some(12_345), ..summed() };
    assert!(DeviceStorage::from_sums(&odd).is_err());
    let none = StorageSums { audio_budget: None, ..summed() };
    assert!(DeviceStorage::from_sums(&none).is_err());
}

/// The next value of `live`.
async fn next(live: &mut coven::LiveQuery<DeviceStorage, DbError>) -> DeviceStorage {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, live.next_inline())
        .await
        .expect("the storage delivers a value")
        .expect("the library stays open")
        .expect("the storage reads")
}

/// The next value of `live` that `done` accepts.
async fn next_until(
    live: &mut coven::LiveQuery<DeviceStorage, DbError>,
    done: impl Fn(&DeviceStorage) -> bool,
) -> DeviceStorage {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let storage = next(live).await;
            if done(&storage) {
                return storage;
            }
        }
    })
    .await
    .expect("the storage reaches the state waited for")
}

/// The bytes of the catalog image cache's rows.
#[cfg(desktop)]
async fn image_bytes(manager: &LibraryManager) -> u64 {
    manager
        .store
        .read(|sql| {
            Ok(sql.query_row("SELECT COALESCE(SUM(bytes), 0) FROM cached_image", [], |row| {
                row.get(0)
            })?)
        })
        .await
        .unwrap()
}

/// A release pinned beside a track played from the cloud and a catalog
/// image: Pinned, Cache and Library hold coven's numbers, Cache with the
/// images beside them.
#[tokio::test]
async fn the_parts_hold_what_coven_and_the_image_cache_measure() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let pinned = uploaded_release(manager, &temp_dir, "pinned", "Pinned", true).await;
    let played = uploaded_release_with_files(
        manager,
        &temp_dir,
        "played",
        "Played",
        &[("played.flac", b"played-bytes")],
        false,
    )
    .await;
    let played_file = release_files(manager, &played).await.remove(0);
    manager.evict_file_for_test(played_file.file).await.unwrap();
    let played_ref = manager.release_file_ref(&played_file.id).await.unwrap();
    manager.store.read_file(&played_ref, &bae_util::test_files::lease()).await.unwrap();
    assert!(!release_files(manager, &pinned).await.is_empty());
    #[cfg(desktop)]
    test.library
        .remote_images()
        .put_for_test("https://example.test/cover.jpg", b"image-bytes")
        .await;

    let local = manager.store.local_bytes().await.unwrap();
    let pinned_bytes = local.namespaces.iter().map(|bytes| bytes.pinned).sum::<u64>();
    let cached_bytes = local.namespaces.iter().map(|bytes| bytes.cached).sum::<u64>();
    assert!(pinned_bytes > 0 && cached_bytes > 0, "{local:?}");

    let mut live = manager.subscribe_device_storage();
    let storage = next_until(&mut live, |_| true).await;
    #[cfg(desktop)]
    let images = image_bytes(manager).await;
    #[cfg(not(desktop))]
    let images = 0;
    #[cfg(desktop)]
    assert!(images > 0, "the catalog image cache holds the image");
    // The library's own files change with every write, so they are only
    // checked to be there.
    let kinds: Vec<_> = storage.parts.iter().map(|part| part.kind).collect();
    assert_eq!(kinds, [StorageKind::Pinned, StorageKind::Cache, StorageKind::Library]);
    assert_eq!(storage.parts[0].bytes, pinned_bytes);
    assert_eq!(storage.parts[1].bytes, cached_bytes + images);
    assert!(storage.parts[2].bytes > 0);
    assert_eq!(storage.cache_limit, CacheLimit::DEFAULT);
    assert!(storage.can_clear_cache);
}

/// Clear Cache empties Cache, catalog images with it, and leaves Pinned; the
/// subscription delivers the change.
#[tokio::test]
async fn clear_cache_empties_the_cache_and_leaves_the_pinned_releases() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    uploaded_release(manager, &temp_dir, "pinned", "Pinned", true).await;
    let played = uploaded_release_with_files(
        manager,
        &temp_dir,
        "played",
        "Played",
        &[("played.flac", b"played-bytes")],
        false,
    )
    .await;
    let played_file = release_files(manager, &played).await.remove(0);
    manager.evict_file_for_test(played_file.file).await.unwrap();
    let played_ref = manager.release_file_ref(&played_file.id).await.unwrap();
    manager.store.read_file(&played_ref, &bae_util::test_files::lease()).await.unwrap();
    #[cfg(desktop)]
    test.library
        .remote_images()
        .put_for_test("https://example.test/cover.jpg", b"image-bytes")
        .await;

    let mut live = manager.subscribe_device_storage();
    let before = next_until(&mut live, |storage| storage.can_clear_cache).await;
    let pinned_before = before.parts.iter().find(|part| part.kind == StorageKind::Pinned).copied();
    assert!(pinned_before.is_some());

    manager.clear_cache().await.unwrap();

    let after = next_until(&mut live, |storage| !storage.can_clear_cache).await;
    assert_eq!(after.cache, 0);
    assert!(after.parts.iter().all(|part| part.kind != StorageKind::Cache));
    assert_eq!(
        after.parts.iter().find(|part| part.kind == StorageKind::Pinned).copied(),
        pinned_before,
        "the pinned release stays"
    );
}

/// Keep up to sets coven's audio budget in one write and the read follows.
#[tokio::test]
async fn set_cache_limit_sets_the_audio_budget() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut live = manager.subscribe_device_storage();
    assert_eq!(next_until(&mut live, |_| true).await.cache_limit, CacheLimit::Gb20);

    manager.set_cache_limit(CacheLimit::Gb1).await.unwrap();

    let storage = next_until(&mut live, |storage| storage.cache_limit == CacheLimit::Gb1).await;
    assert_eq!(storage.cache_limit, CacheLimit::Gb1);
    let local = manager.store.local_bytes().await.unwrap();
    let audio = local
        .namespaces
        .iter()
        .find(|bytes| bytes.namespace == RELEASE_FILES_NAMESPACE)
        .expect("the audio namespace is declared");
    assert_eq!(audio.budget, CacheLimit::Gb1.bytes());
}

/// A budget no choice names fails the subscription's read.
#[tokio::test]
async fn a_budget_no_choice_names_fails_the_subscription() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    manager
        .store
        .call(|sql| Ok(sql.set_cache_budget(RELEASE_FILES_NAMESPACE, 12_345)?))
        .await
        .unwrap();
    let mut live = manager.subscribe_device_storage();
    let first = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, live.next_inline())
        .await
        .expect("the storage delivers a value")
        .expect("the library stays open");
    assert!(first.is_err(), "{first:?}");
}

/// What a reset kept shows as From before the reset until it is deleted, and
/// the subscription delivers each change.
#[tokio::test]
async fn data_from_before_a_reset_shows_until_it_is_deleted() {
    let temp = bae_util::test_files::TestFolder::new();
    let app_dir = bae_app_dir::AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let sources = bae_store::TestSources::system();
    let storage = std::sync::Arc::new(
        coven::MemoryStorage::builder().clock(sources.clock().clone()).build().unwrap(),
    );
    let created = crate::create_library(
        sources.builder(&installation).storage_connector(storage.clone()),
        bae_store::EnteredName::parse("Local Library").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    // A reset loads the library back from storage, so the library has set it up.
    created
        .setup_s3_storage(
            coven::Storage::config(storage.as_ref()),
            "owner-key".into(),
            coven::SecretText::new("owner-secret".into()),
            &|_| (),
        )
        .await
        .unwrap();
    let store = created.store_id();
    created.close().await.unwrap();
    let handle = crate::reset_device(
        sources.builder(&installation).storage_connector(std::sync::Arc::new(storage.for_device())),
        store,
        "Replacement",
        |_| {},
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let library = crate::test_libraries::test_library_from_handle(
        handle,
        temp.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
        &sources,
    )
    .await;
    let manager = library.manager();

    let mut live = manager.subscribe_device_storage();
    let reset = next_until(&mut live, |_| true).await;
    let kept = reset.before_reset.expect("the reset kept the installation it replaced");
    assert!(kept > 0);
    assert_eq!(
        reset.parts.last().copied(),
        Some(StoragePart { kind: StorageKind::BeforeReset, bytes: kept })
    );

    manager.delete_data_from_before_reset().await.unwrap();

    let deleted = next_until(&mut live, |storage| storage.before_reset.is_none()).await;
    assert!(deleted.parts.iter().all(|part| part.kind != StorageKind::BeforeReset));
}

/// Caching a catalog image changes the page's Cache figure once, by the
/// image's row, with no value in between.
#[cfg(desktop)]
#[tokio::test]
async fn caching_an_image_changes_the_cache_figure_once() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut live = manager.subscribe_device_storage();
    let before = next(&mut live).await;

    test.library.remote_images().put_for_test("https://example.test/cover.jpg", b"image").await;

    let after = next_until(&mut live, |storage| storage.cache != before.cache).await;
    let images = image_bytes(manager).await;
    assert!(images > 0, "the image cache keeps a row for the image");
    assert_eq!(after.cache, before.cache + images);
}

/// A write that grows the library's database changes the Library figure
/// once a checkpoint moves the write into the database file.
#[cfg(desktop)]
#[tokio::test]
async fn a_write_that_grows_the_library_changes_the_library_figure_after_a_checkpoint() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    manager.store.checkpoint_for_test().await;
    let mut live = manager.subscribe_device_storage();
    let library = |storage: &DeviceStorage| {
        storage.parts.iter().find(|part| part.kind == StorageKind::Library).unwrap().bytes
    };
    let before = library(&next(&mut live).await);

    for image in 0..200 {
        test.library
            .remote_images()
            .put_for_test(&format!("https://example.test/{image}.jpg"), b"image")
            .await;
    }
    manager.store.checkpoint_for_test().await;

    next_until(&mut live, |storage| library(storage) > before).await;
}
