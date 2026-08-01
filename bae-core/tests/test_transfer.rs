#![cfg(feature = "test-utils")]
//! Integration tests for the bae-layer behavior that rides coven's owned-blob
//! transitions: the make-Remote outbox-snapshot visibility, the host-provided
//! cover blob through coven's local store, the pin/unpin guards and successes
//! (the `TransferProgress` events through coven's cache), the
//! missing-external-source read error `read_release_file_bytes` surfaces, and the
//! `ReleaseUpdated` events coven's completions emit. coven's own suite owns the
//! transition semantics themselves.
//!
//! coven owns the transitions; tests drive them through the manager's coven
//! seams (`coven_make_remote` + the upload drain via `drain_uploads_expecting_work`,
//! `coven_make_local`) over a `SyncManager` connected to an injected cloud home.

use bae_test_support as support;
use support::tracing_init;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{Database, DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::library::{CancellationToken, LibraryEvent, LibraryManager};
use bae_core::storage::transfer::{read_release_file_bytes, TransferProgress, TransferService};
use bae_core::sync::CloudCipher;
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use coven::EncryptionService;
use coven::InMemoryCloudHome;
use coven::StoreDir;
use std::path::Path;
use std::sync::Arc;
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

async fn setup(tmp: &TempDir) -> (Database, LibraryManager) {
    let library_dir = StoreDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let mgr = LibraryManager::open(
        config_handle,
        key_service,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        None,
    )
    .unwrap();
    (mgr.database_for_test(), mgr)
}

/// A manager with a `SyncManager` connected over an injected `InMemoryCloudHome`,
/// sealing blobs under `enc`, and no sync loop behind it — these tests drive the
/// upload drain themselves. After this, `get_cloud_home` is Some. Opaque home
/// (the default at-rest mode), so blobs are keyed hashed and
/// `release_files.cloud_path` stays NULL.
async fn setup_with_cloud(tmp: &TempDir) -> (Database, LibraryManager) {
    let (db, mgr) = setup(tmp).await;
    let cloud = Arc::new(InMemoryCloudHome::new());
    let enc = EncryptionService::from_key([9u8; 32]);
    mgr.connect_test_cloud_home_caller_driven(cloud, CloudCipher::Encrypted(enc))
        .await
        .unwrap();
    (db, mgr)
}

/// Insert a Local release: an album + release with `remote = false`, its originals
/// written under `source_dir`, one `DbFile` per file, and each file registered as
/// a coven user-provided external ref (the in-place files of a Local release).
async fn create_local_release(
    db: &Database,
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
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let (album_id, release_id) = (album.id.clone(), release.id.clone());
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        tokio::fs::write(source_dir.join(name), data).await.unwrap();
        let file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
            bae_core::util::fs::hash_bytes(data),
        );
        mgr.add_file(&file).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    // Register the in-place files as coven external refs (after the rows exist).
    db.register_release_external_refs_for_test(&release_id, &source_dir.to_string_lossy())
        .await
        .unwrap();
    (album_id, release_id, result)
}

/// Get a Remote (cloud-only, not pinned) release whose blobs sit in `cloud`:
/// create it Local, enqueue make-Remote via coven, and drain the uploads so the
/// drain flips the gate Remote (dropping the external refs + deleting the
/// originals, exactly as production does).
async fn create_remote_release(
    db: &Database,
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (_album_id, release_id, named) = create_local_release(db, mgr, source_dir, files).await;
    mgr.coven_make_remote(&release_id, false).await.unwrap();
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

/// Wait for a `ReleaseUpdated` for `release_id` on the manager's event channel,
/// or fail after a bounded number of events.
async fn expect_release_updated(
    rx: &mut tokio::sync::broadcast::Receiver<LibraryEvent>,
    release_id: &str,
) {
    for _ in 0..50 {
        match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(LibraryEvent::ReleaseUpdated { release, .. }))
                if release.summary.id == release_id =>
            {
                return;
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) | Err(_) => break,
        }
    }
    panic!("expected a ReleaseUpdated for {release_id}");
}

// ---------------------------------------------------------------------------
// The make-Remote enqueue is visible in the outbox snapshot before the drain
// ---------------------------------------------------------------------------

/// A Managed import lands Unmanaged-and-uploading: the make-Remote uploads are
/// enqueued through coven, and the Storage Manager / import-progress UI reads
/// them from `outbox_snapshot`. Assert they are visible the instant make-Remote
/// returns — before any byte drains — so the upload shows up immediately rather
/// than only after it finishes.
#[tokio::test]
async fn make_remote_uploads_are_visible_in_snapshot_before_drain() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup_with_cloud(&tmp).await;
    let source = tmp.path().join("src");
    let (_album_id, release_id, named) = create_local_release(
        &db,
        &mgr,
        &source,
        &[("01.flac", b"aaaaaaaa"), ("02.flac", b"bbbbbbbbbbbb")],
    )
    .await;

    // Enqueue make-Remote through the real coven path; do NOT drain.
    mgr.coven_make_remote(&release_id, false).await.unwrap();

    let snap = mgr.outbox_snapshot().await.unwrap();
    assert_eq!(
        snap.total.queued,
        named.len() as u32,
        "every file is queued in the snapshot immediately after make-Remote"
    );
    let group = snap
        .upload_groups
        .iter()
        .find(|group| group.release_id.as_deref() == Some(release_id.as_str()))
        .expect("the uploading release is present in upload_groups");
    assert_eq!(group.progress.queued, named.len() as u32);
}

// ---------------------------------------------------------------------------
// Host-provided cover blob via coven's local store
// ---------------------------------------------------------------------------

/// A cover is a host-provided blob: its bytes go to coven's local store
/// store and coven owns the copy. While the release is Local the cover lives in
/// coven's local store and `read_image_blob` serves its bytes through the image
/// reference — no cloud round-trip and no bae path into coven's store.
#[tokio::test]
async fn test_cover_blob_stored_via_local_files_is_readable() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup(&tmp).await;
    let (_a, release_id, _f) = create_local_release(&db, &mgr, &tmp.path().join("src"), &[]).await;

    // No cover row yet → no bytes.
    assert!(
        support::read_cover_image_blob(&db, &mgr, &release_id)
            .await
            .is_none(),
        "no cover before one is stored"
    );

    // Store the cover bytes and `covers` row in one coven batch, exactly as
    // import / change_cover does.
    let bytes = b"cover-jpeg-bytes";
    mgr.store_library_image_blob(
        &bae_core::db::DbLibraryImage {
            id: release_id.clone(),
            blob_id: format!("{release_id}-cover-blob"),
            image_type: bae_core::db::LibraryImageType::Cover,
            content_type: bae_core::util::content_type::ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            content_hash: bae_core::util::fs::hash_bytes(bytes),
            created_at: chrono::Utc::now(),
        },
        bytes,
    )
    .await
    .unwrap();

    // read_image_blob serves it from coven's local store, byte-for-byte.
    let read = support::read_cover_image_blob(&db, &mgr, &release_id)
        .await
        .expect("cover resolves to bytes once stored");
    assert_eq!(read, bytes, "the stored cover reads back byte-for-byte");
}

// ---------------------------------------------------------------------------
// Pin / unpin guards
// ---------------------------------------------------------------------------

/// Pin rejects a Local release: there is no cloud blob to fetch into the pinned
/// cache.
#[tokio::test]
async fn test_pin_rejects_local_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&db, &mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    let events = collect_progress(
        TransferService::new(mgr.clone())
            .pin_release_task(release_id)
            .0,
    )
    .await;
    assert!(failed(&events), "pin must fail for a local release");
}

/// Unpin rejects a Local release: it has no remote blobs to drop from the cache.
#[tokio::test]
async fn test_unpin_rejects_local_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&db, &mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    assert!(
        mgr.unpin_release(&release_id).await.is_err(),
        "unpin must fail for a local release"
    );
}

// ---------------------------------------------------------------------------
// Pin / unpin success — through coven's cache
// ---------------------------------------------------------------------------

/// Pin a Remote cloud-only release: coven fetches its blobs from the cloud into
/// `storage/pinned/`, so it reads as (Remote, pinned).
#[tokio::test]
async fn test_pin_remote_fetches_into_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        &tmp.path().join("src"),
        &[("a.flac", b"pin-bytes-a"), ("b.flac", b"pin-bytes-b")],
    )
    .await;

    let events = collect_progress(
        TransferService::new(mgr.clone())
            .pin_release_task(release_id.clone())
            .0,
    )
    .await;
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

/// Unpin a Remote pinned release: coven moves its blobs out of `storage/pinned/`
/// into the evictable cache, so it reads as (Remote, not pinned).
#[tokio::test]
async fn test_unpin_remote_drops_from_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        &tmp.path().join("src"),
        &[("a.flac", b"unpin-bytes")],
    )
    .await;

    collect_progress(
        TransferService::new(mgr.clone())
            .pin_release_task(release_id.clone())
            .0,
    )
    .await;
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
// read_release_file_bytes surfaces coven's external-source read failure as an error
// ---------------------------------------------------------------------------

/// A Local release whose external source file has vanished maps to a read error
/// (coven's `ExternalMissing`), not empty bytes or a crash — the "files missing"
/// state the UI surfaces.
#[tokio::test]
async fn test_missing_external_source_maps_to_error() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _f) =
        create_local_release(&db, &mgr, &source_dir, &[("a.flac", b"present")]).await;
    let file = mgr
        .get_files_for_release(&release_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    // The user moved/deleted their file out from under the external ref.
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
// Transition completions emit ReleaseUpdated
// ---------------------------------------------------------------------------

/// Each coven transition completion re-emits the release to bae's subscribers:
/// a drained make-Remote (via coven's `on_root_made_remote` callback) and a
/// make-Local both fire a `LibraryEvent::ReleaseUpdated`, so cached UI details
/// refresh when a release's storage changes. The transitions themselves are
/// coven's; this asserts only the bae-layer event that rides their completion.
#[tokio::test]
async fn transition_completions_emit_release_updated() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _named) =
        create_local_release(&db, &mgr, &source_dir, &[("a.flac", b"round-trip-bytes")]).await;
    let mut events = mgr.subscribe_events();

    // A drained make-Remote completion emits ReleaseUpdated.
    mgr.coven_make_remote(&release_id, false).await.unwrap();
    mgr.drain_uploads_expecting_work().await.unwrap();
    expect_release_updated(&mut events, &release_id).await;

    // A make-Local completion emits ReleaseUpdated.
    let dest = tmp.path().join("brought-back");
    mgr.coven_make_local(
        &release_id,
        dest.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    expect_release_updated(&mut events, &release_id).await;
}
