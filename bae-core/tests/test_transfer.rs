#![cfg(feature = "test-utils")]
//! Integration tests for the bae-side release storage transitions onto coven's
//! owned-blob model: the pin/unpin guards and successes (through coven's cache),
//! the read-bytes durability check, the make-Local rollback windows (cancel +
//! dest failure), the Local↔Remote round-trip, the missing-external-source read
//! error, and the `ReleaseUpdated` events coven's completions emit.
//!
//! coven owns the transitions; tests drive them through coven's free functions
//! (`make_remote` + the upload drain via `process_cloud_uploads_with`,
//! `make_local` via `make_local_for_test`) rather than a live `SyncManager`.

mod support;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{Database, DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::encryption::EncryptionService;
use bae_core::library::{CancellationToken, LibraryEvent, LibraryManager};
use bae_core::library_dir::LibraryDir;
use bae_core::storage::local::transfer::{
    read_release_file_bytes, TransferProgress, TransferService,
};
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::path::Path;
use std::sync::{Arc, RwLock};
use support::MockCloudHome;
use tempfile::TempDir;
use uuid::Uuid;

fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

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
    let library_dir = LibraryDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let db = Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let mgr = LibraryManager::new(
        db.clone(),
        library_dir,
        config_handle,
        key_service,
        Arc::new(bae_core::clock::SystemClock),
        Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );
    (db, mgr)
}

/// A manager with a `MockCloudHome` + encryption injected (the cloud read/write
/// paths resolve without a live SyncManager) and the sync pipeline modelled as
/// running. Opaque home (the default at-rest mode), so blobs are keyed hashed and
/// `release_files.cloud_path` stays NULL.
async fn setup_with_cloud(
    tmp: &TempDir,
) -> (
    Database,
    LibraryManager,
    Arc<MockCloudHome>,
    EncryptionService,
) {
    let (db, mut mgr) = setup(tmp).await;
    let cloud = Arc::new(MockCloudHome::new());
    let enc = EncryptionService::new_with_key(&[9u8; 32]);
    mgr.set_cloud_override(cloud.clone(), enc.clone());
    mgr.set_force_sync_ready();
    (db, mgr, cloud, enc)
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
    let artist_id = "test-transfer-artist";
    let _ = mgr
        .insert_artist(&bae_core::db::DbArtist {
            id: artist_id.to_string(),
            name: "Artist Name".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        })
        .await;
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
    cloud: &MockCloudHome,
    enc_svc: EncryptionService,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (_album_id, release_id, named) = create_local_release(db, mgr, source_dir, files).await;
    mgr.make_remote_for_test(&release_id, false).await.unwrap();
    let enc = RwLock::new(enc_svc);
    let count = mgr.process_cloud_uploads_with(cloud, &enc).await.unwrap();
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
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
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
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
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
// read_release_file_bytes durability + missing-external-source error
// ---------------------------------------------------------------------------

/// `read_release_file_bytes` aborts when the bytes on disk are shorter than the
/// declared `file_size` (coven's external-ref validate-on-read catches the size
/// mismatch; the read fails before any caller trusts the bytes).
#[tokio::test]
async fn test_read_release_file_bytes_rejects_short_read() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr) = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _f) = create_local_release(&db, &mgr, &source_dir, &[]).await;

    // A file whose declared size exceeds the bytes actually on disk.
    let actual = b"short";
    tokio::fs::write(source_dir.join("track.flac"), actual)
        .await
        .unwrap();
    let file = DbFile::new(
        &release_id,
        "track.flac",
        (actual.len() + 100) as i64,
        ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    mgr.add_file(&file).await.unwrap();
    db.register_release_external_refs_for_test(&release_id, &source_dir.to_string_lossy())
        .await
        .unwrap();

    let result = read_release_file_bytes(&file, &mgr).await;
    assert!(result.is_err(), "short read must fail the length check");
}

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
// make-Local durability: cancel + abort roll back and queue no deletes
// ---------------------------------------------------------------------------

/// A cancelled make-Local rolls back: nothing is written at the new path, no
/// cloud delete is queued, and the release stays Remote.
#[tokio::test]
async fn test_make_local_cancelled_rolls_back_and_stays_remote() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
        &tmp.path().join("src"),
        &[("a.flac", b"aaaa"), ("b.flac", b"bbbb")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    // Cancel before the transition runs: coven aborts at the first blob, rolls
    // back any partial copy, and leaves the release Remote.
    let token = CancellationToken::new();
    token.cancel();
    mgr.make_local_for_test(&release_id, new_path.to_str().unwrap(), &token)
        .await
        .unwrap();

    for name in ["a.flac", "b.flac"] {
        assert!(!new_path.join(name).exists(), "{name} rolled back");
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
}

/// A make-Local whose destination can't be written (the path is a file) fails,
/// queues no delete, and the release stays Remote.
#[tokio::test]
async fn test_make_local_abort_on_dest_failure_queues_no_deletes() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
        &tmp.path().join("src"),
        &[("a.flac", b"aaaa")],
    )
    .await;

    // Destination is a FILE, so writing `dest/a.flac` under it fails.
    let new_path = tmp.path().join("dest_is_a_file");
    tokio::fs::write(&new_path, b"blocker").await.unwrap();

    let result = mgr
        .make_local_for_test(
            &release_id,
            new_path.to_str().unwrap(),
            &CancellationToken::new(),
        )
        .await;
    assert!(
        result.is_err(),
        "make-Local must fail when dest can't be written"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Round-trip + completion events
// ---------------------------------------------------------------------------

/// A full round-trip: Local → Remote (drain flips the gate) → Local (materialize
/// + retract, files back at the chosen folder) → Remote again. Each completion
/// emits a `ReleaseUpdated`.
#[tokio::test]
async fn test_round_trip_make_remote_make_local_make_remote() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _named) =
        create_local_release(&db, &mgr, &source_dir, &[("a.flac", b"round-trip-bytes")]).await;
    let mut events = mgr.subscribe_events();

    // Local → Remote: enqueue + drain. The drain flips the gate and fires
    // on_root_made_remote → ReleaseUpdated.
    mgr.make_remote_for_test(&release_id, false).await.unwrap();
    let enc_lock = RwLock::new(enc);
    mgr.process_cloud_uploads_with(cloud.as_ref(), &enc_lock)
        .await
        .unwrap();
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "Remote after make_remote"
    );
    expect_release_updated(&mut events, &release_id).await;

    // Remote → Local: materialize back to a chosen folder.
    let dest = tmp.path().join("brought-back");
    mgr.make_local_for_test(
        &release_id,
        dest.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false),
        "Local after make_local"
    );
    assert_eq!(
        tokio::fs::read(dest.join("a.flac")).await.unwrap(),
        b"round-trip-bytes",
        "the file is materialized back at the chosen folder"
    );
    expect_release_updated(&mut events, &release_id).await;

    // Local → Remote again: the external refs registered by make_local let the
    // re-upload read from the new folder.
    mgr.make_remote_for_test(&release_id, false).await.unwrap();
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc_lock)
        .await
        .unwrap();
    assert_eq!(count, 1, "the file re-uploads on the second make_remote");
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "Remote again after the round-trip"
    );
}
