#![cfg(feature = "test-utils")]
//! Integration tests for the bae-side release storage transitions: the pin/unpin
//! guards and successes (through coven's cache), the read-bytes durability check,
//! the unmanage rollback windows (a verified destination copy must exist before
//! any delete is queued — across a mid-transfer cancel and a write failure), and
//! the managed cloud-key layout (readable on a browsable home, hashed on an
//! opaque one). The full manage/unmanage cloud lifecycle — the observer-driven
//! managed flip, cache reads, and missing/short-blob hard errors — lives in
//! `test_storage_state_machine.rs`.

mod support;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{Database, DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::encryption::EncryptionService;
use bae_core::library::{CancellationToken, LibraryManager};
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
/// Storage state is the 2-way `managed` fact; pinned is the orthogonal coven-cache
/// property. Mirrors what the UI shows.
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

/// A manager with a `MockCloudHome` + encryption injected (cloud read/write paths
/// resolve without a live SyncManager) and the sync pipeline modelled as running
/// (manage gates on `is_sync_ready`). Opaque home (the default at-rest mode).
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

/// Like [`setup_with_cloud`], but the home is browsable: blobs are stored in the
/// clear at readable paths, so managed cloud keys are id-structured rather than
/// hashed.
async fn setup_with_browsable_cloud(
    tmp: &TempDir,
) -> (
    Database,
    LibraryManager,
    Arc<MockCloudHome>,
    EncryptionService,
) {
    let (db, mgr, cloud, enc) = setup_with_cloud(tmp).await;
    mgr.set_home_storage(bae_core::config::HomeStorage::Browsable);
    (db, mgr, cloud, enc)
}

/// Insert an Unmanaged release: an album + release with `managed = false`, its
/// originals written under `source_dir`, the `release_unmanaged_source` row set,
/// and one `DbFile` per file. No uploads queued. Returns (album_id, release_id,
/// files).
async fn create_unmanaged_release(
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> (String, String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    let artist_id = "test-transfer-artist";
    let _ = mgr
        .insert_artist(&bae_core::db::DbArtist {
            id: artist_id.to_string(),
            name: "Test Artist".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        })
        .await;
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Transfer Test Album".to_string(),
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
        managed: false,
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
    mgr.set_release_unmanaged_path(&release_id, &source_dir.to_string_lossy())
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
    (album_id, release_id, result)
}

/// Get a Managed (cloud-only, not pinned) release whose blobs sit in `cloud`:
/// insert it Unmanaged, manage it (pin = false), and drain the uploads so the
/// observer flips it Managed. A later pin/unmanage reads the blobs back through
/// coven's cache.
async fn create_managed_cloud_only_release(
    mgr: &LibraryManager,
    cloud: &MockCloudHome,
    enc_svc: EncryptionService,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (_album_id, release_id, named) = create_unmanaged_release(mgr, source_dir, files).await;
    mgr.manage_release(&release_id, false).await.unwrap();
    let enc = RwLock::new(enc_svc);
    let count = mgr.process_cloud_uploads_with(cloud, &enc).await.unwrap();
    assert_eq!(count, named.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Managed, false),
        "Managed (cloud-only) after the drain"
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

// ---------------------------------------------------------------------------
// Pin / unpin guards
// ---------------------------------------------------------------------------

/// Pin rejects an unmanaged release: there is no cloud blob to fetch into the
/// pinned cache.
#[tokio::test]
async fn test_pin_rejects_unmanaged_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr) = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_unmanaged_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    let events = collect_progress(
        TransferService::new(mgr.clone())
            .pin_release_task(release_id)
            .0,
    )
    .await;
    assert!(failed(&events), "pin must fail for an unmanaged release");
}

/// Unpin rejects an unmanaged release: it has no managed blobs to drop from the
/// cache.
#[tokio::test]
async fn test_unpin_rejects_unmanaged_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr) = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_unmanaged_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    assert!(
        mgr.unpin_release(&release_id).await.is_err(),
        "unpin must fail for an unmanaged release"
    );
}

// ---------------------------------------------------------------------------
// Pin / unpin success — through coven's cache
// ---------------------------------------------------------------------------

/// Pin a managed cloud-only release: coven fetches its blobs from the cloud into
/// `storage/pinned/`, so it reads as (Managed, pinned).
#[tokio::test]
async fn test_pin_managed_fetches_into_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_managed_cloud_only_release(
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
        (ReleaseStorageState::Managed, true)
    );
}

/// Unpin a managed pinned release: coven moves its blobs out of `storage/pinned/`
/// into the evictable cache, so it reads as (Managed, not pinned). The cloud copy
/// is untouched.
#[tokio::test]
async fn test_unpin_managed_drops_from_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_managed_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc,
        &tmp.path().join("src"),
        &[("a.flac", b"unpin-bytes")],
    )
    .await;

    // Pin first, then unpin.
    collect_progress(
        TransferService::new(mgr.clone())
            .pin_release_task(release_id.clone())
            .0,
    )
    .await;
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Managed, true)
    );

    mgr.unpin_release(&release_id).await.unwrap();
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Managed, false)
    );
}

// ---------------------------------------------------------------------------
// read_release_file_bytes durability check
// ---------------------------------------------------------------------------

/// `read_release_file_bytes` aborts when the bytes on disk are shorter than the
/// declared `file_size` (a short read must fail before any transition queues a
/// delete).
#[tokio::test]
async fn test_read_release_file_bytes_rejects_short_read() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr) = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _f) = create_unmanaged_release(&mgr, &source_dir, &[]).await;

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

    let source = mgr
        .get_release_unmanaged_source(&release_id)
        .await
        .unwrap()
        .unwrap();
    let result = read_release_file_bytes(Some(&source), &file, &mgr).await;
    assert!(result.is_err(), "short read must fail the length check");
}

// ---------------------------------------------------------------------------
// Unmanage durability: cancel + abort roll back and queue no deletes
// ---------------------------------------------------------------------------

/// Cancelling an unmanage after the first file is written rolls back: the partial
/// copy is deleted, no cloud delete is queued, and the release stays Managed.
#[tokio::test]
async fn test_unmanage_cancelled_after_first_file_rolls_back_and_stays_managed() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_managed_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc,
        &tmp.path().join("src"),
        &[("a.flac", b"aaaa"), ("b.flac", b"bbbb")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    let token = CancellationToken::new();
    let mut rx = TransferService::new(mgr.clone()).unmanage_release(
        release_id.clone(),
        new_path.to_str().unwrap().to_string(),
        token.clone(),
    );
    while let Some(p) = rx.recv().await {
        match p {
            TransferProgress::FileProgress {
                file_index: 0,
                percent: 100,
                ..
            } => token.cancel(),
            TransferProgress::Complete { .. } | TransferProgress::Failed { .. } => break,
            _ => {}
        }
    }

    // Nothing left at the new path; release stays Managed (cloud-only); no deletes.
    for name in ["a.flac", "b.flac"] {
        assert!(!new_path.join(name).exists(), "{name} rolled back");
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Managed, false)
    );
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
}

/// An unmanage whose destination can't be created (the path is a file) fails
/// before any write, queues no delete, and the release stays Managed.
#[tokio::test]
async fn test_unmanage_abort_on_dest_failure_queues_no_deletes() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud, enc) = setup_with_cloud(&tmp).await;
    let release_id = create_managed_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc,
        &tmp.path().join("src"),
        &[("a.flac", b"aaaa")],
    )
    .await;

    // Destination is a FILE, so create_dir_all on it fails and unmanage aborts.
    let new_path = tmp.path().join("dest_is_a_file");
    tokio::fs::write(&new_path, b"blocker").await.unwrap();

    let events = collect_progress(TransferService::new(mgr.clone()).unmanage_release(
        release_id.clone(),
        new_path.to_str().unwrap().to_string(),
        CancellationToken::new(),
    ))
    .await;
    assert!(
        failed(&events),
        "unmanage must fail when dest can't be made"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Managed, false)
    );
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Managed cloud-key layout: readable on a browsable home, hashed on opaque
// ---------------------------------------------------------------------------

/// Managing into a BROWSABLE home stores `storage/{album_id}/{release_id}/{name}`
/// on each `release_files.cloud_path`, enqueues the outbox upload under that same
/// key, and the read path resolves to it — the synced row and the cloud object
/// agree on the id-structured, collision-free path (not the hashed
/// `storage/ab/cd/{id}`).
#[tokio::test]
async fn test_manage_browsable_stores_readable_cloud_path() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, _cloud, _enc) = setup_with_browsable_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (album_id, release_id, originals) =
        create_unmanaged_release(&mgr, &source_dir, &[("a.flac", b"aa"), ("b.flac", b"bb")]).await;

    // Manage cloud-only: enqueues uploads and sets each file's cloud_path. No live
    // sync loop drains here, so the release stays Unmanaged; only the file rows and
    // outbox are written.
    mgr.manage_release(&release_id, false).await.unwrap();

    // The row carries the NAMESPACE-RELATIVE readable key (coven prepends the
    // `storage/` audio namespace when it reads/writes the blob).
    let release_prefix = format!("{album_id}/{release_id}/");
    let files = mgr.get_files_for_release(&release_id).await.unwrap();
    for file in &files {
        assert_eq!(
            file.cloud_path.as_deref(),
            Some(format!("{release_prefix}{}", file.original_filename).as_str()),
            "browsable file {} carries the namespace-relative readable cloud_path",
            file.original_filename
        );
    }

    // The outbox carries the FULL object key — the `storage/` namespace prepended
    // to the row's relative cloud_path (and never the hashed `storage/ab/cd/{id}`).
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), originals.len());
    for file in &files {
        let full_key = file.cloud_key();
        assert!(
            uploads.iter().any(|u| u.cloud_key == full_key),
            "outbox has {full_key}"
        );
        assert_ne!(
            full_key,
            bae_core::storage::local::storage_path(&file.id),
            "a browsable key is not the hashed storage_path"
        );
    }
}

/// Managing into an OPAQUE home leaves every `release_files.cloud_path` NULL and
/// enqueues each upload under the hashed `storage_path(file_id)`.
#[tokio::test]
async fn test_manage_opaque_leaves_cloud_path_null() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, _cloud, _enc) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_album_id, release_id, _originals) =
        create_unmanaged_release(&mgr, &source_dir, &[("a.flac", b"aa")]).await;

    mgr.manage_release(&release_id, false).await.unwrap();

    let files = mgr.get_files_for_release(&release_id).await.unwrap();
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    for file in &files {
        assert_eq!(file.cloud_path, None, "opaque file leaves cloud_path NULL");
        let hashed = bae_core::storage::local::storage_path(&file.id);
        assert!(
            uploads.iter().any(|u| u.cloud_key == hashed),
            "opaque upload enqueued under the hashed storage_path {hashed}"
        );
    }
}

/// A cover on a BROWSABLE home keys its `BlobRef.cloud_path` id-structured
/// (`{album_id}/{release_id}/cover.jpg`); on an OPAQUE home it stays None.
#[tokio::test]
async fn test_cover_blob_ref_cloud_path_browsable_vs_opaque() {
    tracing_init();
    use bae_core::db::LibraryImageType;
    use bae_core::sync::blob_source::BaeBlobSource;
    use coven::blob::BlobSource;

    // Browsable: the cover keys by id.
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud, _enc) = setup_with_browsable_cloud(&tmp).await;
    let (album_id, release_id, _f) =
        create_unmanaged_release(&mgr, &tmp.path().join("src"), &[]).await;
    let expected_cover = format!("{album_id}/{release_id}/cover.jpg");

    let cloud_path = mgr
        .cover_cloud_path_for_test(&release_id, &ContentType::Jpeg)
        .await;
    assert_eq!(cloud_path.as_deref(), Some(expected_cover.as_str()));
    mgr.upsert_library_image(&bae_core::db::DbLibraryImage {
        id: release_id.clone(),
        image_type: LibraryImageType::Cover,
        content_type: ContentType::Jpeg,
        file_size: 10,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path,
        created_at: Utc::now(),
    })
    .await
    .unwrap();

    let source = BaeBlobSource::new(LibraryDir::new(tmp.path()));
    let refs = db
        .coven_db()
        .call(move |conn| {
            source
                .blobs_in_db(conn)
                .map_err(coven::database::DbError::from)
        })
        .await
        .unwrap();
    let cover_ref = refs
        .iter()
        .find(|r| r.id == release_id)
        .expect("cover blob ref present");
    assert_eq!(cover_ref.namespace, "images");
    assert_eq!(
        cover_ref.cloud_path.as_deref(),
        Some(expected_cover.as_str())
    );

    // Opaque: the cover carries no readable path.
    let tmp2 = TempDir::new().unwrap();
    let (db2, mgr2, _cloud2, _enc2) = setup_with_cloud(&tmp2).await;
    let (_a2, release_id2, _f2) =
        create_unmanaged_release(&mgr2, &tmp2.path().join("src"), &[]).await;
    let opaque_path = mgr2
        .cover_cloud_path_for_test(&release_id2, &ContentType::Jpeg)
        .await;
    assert_eq!(opaque_path, None, "opaque cover stores no cloud_path");
    mgr2.upsert_library_image(&bae_core::db::DbLibraryImage {
        id: release_id2.clone(),
        image_type: LibraryImageType::Cover,
        content_type: ContentType::Jpeg,
        file_size: 10,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        created_at: Utc::now(),
    })
    .await
    .unwrap();
    let source2 = BaeBlobSource::new(LibraryDir::new(tmp2.path()));
    let refs2 = db2
        .coven_db()
        .call(move |conn| {
            source2
                .blobs_in_db(conn)
                .map_err(coven::database::DbError::from)
        })
        .await
        .unwrap();
    let cover_ref2 = refs2
        .iter()
        .find(|r| r.id == release_id2)
        .expect("opaque cover blob ref present");
    assert_eq!(cover_ref2.cloud_path, None);
}
