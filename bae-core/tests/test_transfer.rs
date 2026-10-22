#![cfg(feature = "test-utils")]
//! File attachment, uploads, pins, and local copies through the library manager.
//! Coven uses MemoryStorage for cloud reads and writes; bae owns the release
//! transitions, their progress, and their subscribed storage values.

use bae_test_support as support;
use support::tracing_init;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{
    DbAlbum, DbFile, DbRelease, SortDirection, StorageFilter, StorageSortCriterion,
    StorageSortField,
};
use bae_core::library::{
    AppServices, CancellationToken, LibraryManager, StorageBrowseSubscription, StorageBrowseView,
};
use bae_core::storage::transfer::{read_release_file_bytes, TransferProgress, TransferService};
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// (storage state, pinned) for a release — the post-transition assertion target.
async fn storage(mgr: &LibraryManager, release_id: &str) -> (ReleaseStorageState, bool) {
    let s = mgr
        .find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap();
    (s.storage_state, s.pinned)
}

async fn setup(tmp: &TempDir) -> LibraryManager {
    support::open_test_library(tmp.path()).await.0
}

async fn setup_with_cloud(tmp: &TempDir) -> LibraryManager {
    let (manager, _, storage) =
        support::storage_library(tmp.path(), bae_core::providers::Providers::offline()).await;
    support::connect_storage(&manager, &storage).await;
    manager
}

/// Insert a Local release: its originals under `source_dir`, one `DbFile` per
/// file, and each original path in bae's device-local table.
async fn create_local_release(
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> (String, String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    let artist_id = bae_test_support::test_uuid("test-transfer-artist");
    mgr.insert_artist(&bae_core::db::DbArtist {
        id: artist_id.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    })
    .await
    .unwrap();
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Album Title".to_string(),
        artist_id: artist_id.to_string(),
        year: Some(2024),
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_core::pressing::Pressing::blank(),
        draft_from_tags: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let (album_id, release_id) = (album.id.clone(), release.id.clone());
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[])
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        let path = source_dir.join(name);
        tokio::fs::write(&path, data).await.unwrap();
        let file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
        );
        mgr.add_external_file_for_test(&file, &path).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    (album_id, release_id, result)
}

/// Get a Remote (cloud-only, not pinned) release whose files sit in `cloud`:
/// create it Local, attach its files, and finish the uploads.
async fn create_remote_release(
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (_album_id, release_id, named) = create_local_release(mgr, source_dir, files).await;
    mgr.upload_release(&release_id, false).await.unwrap();
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, named.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "Remote (cloud-only) after the drain"
    );
    release_id
}

/// Drain a transfer receiver to its terminal event.
async fn collect_progress(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TransferProgress>,
) -> Vec<TransferProgress> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let terminal = matches!(
            event,
            TransferProgress::Complete { .. } | TransferProgress::Failed { .. }
        );
        events.push(event);
        if terminal {
            break;
        }
    }
    events
}

fn failed(events: &[TransferProgress]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. }))
}

fn completed(events: &[TransferProgress]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. }))
}

async fn expect_storage_state(
    subscription: &StorageBrowseSubscription,
    release_id: &str,
    expected: ReleaseStorageState,
) {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), subscription.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves");
        if value
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .any(|row| row.release.id == release_id && row.release.storage_state == expected)
        {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// The upload enqueue is visible in the outbox snapshot before the drain
// ---------------------------------------------------------------------------

/// A Cloud import attaches its files and queues their uploads. The uploads are
/// enqueued through coven, and the Storage Manager / import-progress UI reads
/// them from `outbox_snapshot`. Assert they are visible the instant upload
/// returns — before any byte drains — so the upload shows up immediately rather
/// than only after it finishes.
#[tokio::test]
async fn uploads_are_visible_in_snapshot_before_drain() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source = tmp.path().join("src");
    let (_album_id, release_id, named) = create_local_release(
        &mgr,
        &source,
        &[("01.flac", b"aaaaaaaa"), ("02.flac", b"bbbbbbbbbbbb")],
    )
    .await;

    // Enqueue upload through the real coven path; do NOT drain.
    mgr.upload_release(&release_id, false).await.unwrap();

    let snap = mgr.outbox_snapshot().await.unwrap();
    assert_eq!(
        snap.total.queued,
        named.len() as u32,
        "every file is queued in the snapshot immediately after upload"
    );
    let group = snap
        .upload_groups
        .iter()
        .find(|group| group.release_id == release_id)
        .expect("the uploading release is present in upload_groups");
    assert_eq!(group.progress.queued, named.len() as u32);
}

#[tokio::test]
async fn durable_outbox_subscription_emits_enqueue_without_a_sync_cycle() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let mut values = mgr.subscribe_outbox_values();
    let source = tmp.path().join("live-outbox");
    let (_album_id, release_id, _) =
        create_local_release(&mgr, &source, &[("track.flac", b"audio")]).await;

    mgr.upload_release(&release_id, false).await.unwrap();

    loop {
        tokio::time::timeout(std::time::Duration::from_secs(5), values.changed())
            .await
            .expect("durable outbox commit wakes the subscription")
            .expect("outbox subscription remains open");
        let current = values.borrow_and_update();
        let snapshot = current
            .as_ref()
            .expect("outbox subscription published a value")
            .as_ref()
            .unwrap_or_else(|error| panic!("outbox projection failed: {error}"));
        if snapshot
            .upload_groups
            .iter()
            .any(|group| group.release_id == release_id)
        {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Host-provided cover file via coven's local store
// ---------------------------------------------------------------------------

/// A cover is a host-provided file: its bytes go to coven's local store
/// store and coven owns the copy. While the release is Local the cover lives in
/// coven's local store and `read_image_bytes` serves its bytes through the image
/// reference — no cloud round-trip and no bae path into coven's store.
#[tokio::test]
async fn test_cover_file_stored_via_local_files_is_readable() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) = create_local_release(&mgr, &tmp.path().join("src"), &[]).await;

    // No cover row yet → no bytes.
    assert!(
        support::read_cover_image_bytes(&mgr, &release_id)
            .await
            .is_none(),
        "no cover before one is stored"
    );

    // Store the cover bytes and `covers` row in one coven batch, exactly as
    // import / change_cover does.
    let bytes = b"cover-jpeg-bytes";
    mgr.store_library_image(
        &bae_core::db::DbLibraryImage {
            id: release_id.clone(),
            image_type: bae_core::db::LibraryImageType::Cover,
            content_type: bae_core::util::content_type::ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            created_at: chrono::Utc::now(),
            revision: uuid::Uuid::new_v4().to_string(),
        },
        bytes,
    )
    .await
    .unwrap();

    // read_image_bytes serves it from coven's local store, byte-for-byte.
    let read = support::read_cover_image_bytes(&mgr, &release_id)
        .await
        .expect("cover resolves to bytes once stored");
    assert_eq!(read, bytes, "the stored cover reads back byte-for-byte");
}

// ---------------------------------------------------------------------------
// Pin / unpin guards
// ---------------------------------------------------------------------------

/// Pin rejects a Local release: there is no cloud file to fetch into the pinned
/// cache.
#[tokio::test]
async fn test_pin_rejects_local_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    let events = TransferService::new(mgr.clone())
        .pin_release(release_id, |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert!(failed(&events), "pin must fail for a local release");
}

/// Unpin rejects a Local release: it has no remote files to drop from the cache.
#[tokio::test]
async fn test_unpin_rejects_local_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    assert!(
        mgr.unpin_release(&release_id).await.is_err(),
        "unpin must fail for a local release"
    );
}

// ---------------------------------------------------------------------------
// Pin / unpin success — through coven's cache
// ---------------------------------------------------------------------------

/// Pin a Remote cloud-only release: coven fetches its files from the cloud into
/// the pinned cache, so it reads as (Remote, pinned).
#[tokio::test]
async fn test_pin_remote_fetches_into_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &mgr,
        &tmp.path().join("src"),
        &[("a.flac", b"pin-bytes-a"), ("b.flac", b"pin-bytes-b")],
    )
    .await;

    let events = TransferService::new(mgr.clone())
        .pin_release(release_id.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert!(completed(&events) && !failed(&events), "pin succeeds");
    assert!(
        matches!(events.first(), Some(TransferProgress::Started)),
        "pin starts"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TransferProgress::Progress { progress }
                if progress.bytes_done == 0 && progress.bytes_total == 22
        )),
        "pin reports known byte total"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TransferProgress::Progress { progress }
                if progress.bytes_done == 22 && progress.bytes_total == 22
        )),
        "pin reports completed bytes before completion"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true)
    );
}

/// Unpin a Remote pinned release: coven moves its files out of the pinned cache
/// into the evictable cache, so it reads as (Remote, not pinned).
#[tokio::test]
async fn test_unpin_remote_drops_from_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id =
        create_remote_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"unpin-bytes")]).await;

    TransferService::new(mgr.clone())
        .pin_release(release_id.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true)
    );

    mgr.unpin_release(&release_id).await.unwrap();
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
}

// ---------------------------------------------------------------------------
// read_release_file_bytes surfaces a missing original as an error
// ---------------------------------------------------------------------------

/// Reading a Local release whose original file vanished returns an error.
#[tokio::test]
async fn test_missing_external_source_maps_to_error() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _f) =
        create_local_release(&mgr, &source_dir, &[("a.flac", b"present")]).await;
    let file = mgr
        .get_files_for_release(&release_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    // The user moved or deleted their original file.
    tokio::fs::remove_file(source_dir.join("a.flac"))
        .await
        .unwrap();

    let result = read_release_file_bytes(&file, &mgr).await;
    assert!(
        result.is_err(),
        "a vanished external source must surface a read error"
    );
}

// ---------------------------------------------------------------------------
// Transition completions deliver new storage values
// ---------------------------------------------------------------------------

/// A drained upload and audio detachment both change the subscribed Storage
/// Manager value without a separate library event or another read from the UI.
#[tokio::test]
async fn transition_completions_deliver_storage_values() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _named) =
        create_local_release(&mgr, &source_dir, &[("a.flac", b"round-trip-bytes")]).await;
    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let values = services.subscribe_storage_browse(
        &tokio::runtime::Handle::current(),
        StorageBrowseView {
            sort: StorageSortCriterion {
                field: StorageSortField::AlbumTitle,
                direction: SortDirection::Ascending,
            },
            filter: StorageFilter::All,
            windows: [bae_core::library::LibraryPageWindow {
                offset: 0,
                limit: 50,
            }]
            .into_iter()
            .collect(),
        },
    );
    expect_storage_state(&values, &release_id, ReleaseStorageState::Local).await;

    mgr.upload_release(&release_id, false).await.unwrap();
    mgr.drain_uploads_expecting_work().await.unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::Remote).await;

    let dest = tmp.path().join("brought-back");
    mgr.detach_release_audio_into(
        &release_id,
        dest.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::Local).await;
}
