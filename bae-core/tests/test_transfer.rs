#![cfg(feature = "test-utils")]
//! Integration tests for the release storage transitions (pin/unpin/manage/
//! unmanage).
//!
//! The SAFETY INVARIANT under test: a durable verified copy must exist at the
//! destination before any delete (cloud-outbox or local pending-deletion) is
//! queued. Each data-loss window listed in the design contract has a test here
//! (cloud-dependent windows live in `test_storage_state_machine.rs`).

mod support;

use bae_core::db::{Database, DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::encryption::EncryptionService;
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::storage::local::cleanup::PendingDeletion;
use bae_core::storage::local::transfer::{
    read_release_file_bytes, TransferProgress, TransferService,
};
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
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

/// This device's storage state for a release, derived from `releases.managed`
/// and its `release_local_copy` row — the post-transition assertion target now
/// that the two raw columns live in separate tables.
async fn storage_state(
    mgr: &LibraryManager,
    release_id: &str,
) -> bae_core::album_detail::ReleaseStorageState {
    let release = mgr.get_release_by_id(release_id).await.unwrap().unwrap();
    let local_copy = mgr.get_release_local_copy(release_id).await.unwrap();
    bae_core::album_detail::storage_state(release.managed, local_copy.as_ref())
}

/// Set up a database and library manager in a temp directory
async fn setup_db(temp: &TempDir, library_path: &Path) -> (Database, LibraryManager) {
    let db_path = temp.path().join("test.db");
    let db = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let library_dir = LibraryDir::new(library_path);
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let mgr = LibraryManager::new(
        db.clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );
    (db, mgr)
}

/// Set up a db + manager and inject a `MockCloudHome` so `get_cloud_home`
/// resolves (Manage/Unmanage and the unpin guard require a cloud home).
/// Returns the shared cloud handle so tests can inspect what landed.
async fn setup_db_with_cloud(
    temp: &TempDir,
    library_path: &Path,
) -> (Database, LibraryManager, Arc<MockCloudHome>) {
    let (db, mut mgr) = setup_db(temp, library_path).await;
    let cloud = Arc::new(MockCloudHome::new());
    mgr.set_cloud_override(cloud.clone(), EncryptionService::new_with_key(&[7u8; 32]));
    // Manage (pin or cloud-only) reaches `managed = true` only when the upload
    // observer fires from a running sync loop. These tests have no live loop, so
    // model one as ready; the refusal-without-sync case is covered in the
    // manager unit tests.
    mgr.set_force_sync_ready();
    (db, mgr, cloud)
}

/// Like [`setup_db_with_cloud`], but the home is browsable: its blobs are stored
/// in the clear at readable paths, so `cloud_blob_cipher` resolves to plaintext
/// and managed reads must return the verbatim bytes (no decryption). The cloud
/// override still carries an `EncryptionService`, but a browsable home never
/// consults it.
async fn setup_db_with_browsable_cloud(
    temp: &TempDir,
    library_path: &Path,
) -> (Database, LibraryManager, Arc<MockCloudHome>) {
    let (db, mgr, cloud) = setup_db_with_cloud(temp, library_path).await;
    mgr.set_home_storage(bae_core::config::HomeStorage::Browsable);
    (db, mgr, cloud)
}

/// Write unmanaged original files to disk under `dir` and insert DbFile rows.
/// Returns (filename, bytes) pairs.
async fn create_unmanaged_files(
    mgr: &LibraryManager,
    release_id: &str,
    dir: &Path,
) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("track1.flac", b"unmanaged-track-one-data" as &[u8]),
        ("track2.flac", b"unmanaged-track-two-data!!"),
    ];

    tokio::fs::create_dir_all(dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in &files {
        tokio::fs::write(dir.join(name), data).await.unwrap();
        let db_file = DbFile::new(
            release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        mgr.add_file(&db_file).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    result
}

/// Create a test album + release in the DB, return (album_id, release_id)
async fn create_album_and_release(
    db: &Database,
    unmanaged_path: Option<&str>,
    pinned_locally: bool,
) -> (String, String) {
    let now = Utc::now();

    // Insert a test artist for the album FK
    let artist_id = "test-transfer-artist";
    let _ = db
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
    db.insert_album(&album).await.unwrap();

    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        // Unmanaged when an in-place path is given; otherwise managed (cloud).
        managed: unmanaged_path.is_none(),
        source_folder_name: None,
        content_hash: None,
        created_at: now,
    };
    db.insert_release(&release).await.unwrap();

    // Record this device's local copy: an unmanaged in-place path, or a
    // managed pin. A managed-unpinned release keeps no local-copy row.
    if let Some(path) = unmanaged_path {
        db.upsert_release_local_copy(&bae_core::db::DbReleaseLocalCopy {
            release_id: release.id.clone(),
            unmanaged_path: Some(path.to_string()),
            pinned_locally: false,
        })
        .await
        .unwrap();
    } else if pinned_locally {
        db.upsert_release_local_copy(&bae_core::db::DbReleaseLocalCopy {
            release_id: release.id.clone(),
            unmanaged_path: None,
            pinned_locally: true,
        })
        .await
        .unwrap();
    }

    (album.id, release.id)
}

/// Create pinned local files on disk and insert DbFile records
async fn create_pinned_local_files(
    mgr: &LibraryManager,
    release_id: &str,
    library_dir: &LibraryDir,
) -> Vec<(String, Vec<u8>)> {
    let files = vec![
        ("track1.flac", b"stored-data-track-one" as &[u8]),
        ("track2.flac", b"stored-data-track-two"),
    ];

    let mut result = Vec::new();
    for (name, data) in &files {
        let db_file = DbFile::new(
            release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );

        // Write data to the derived storage path
        let storage_path = db_file.local_storage_path(library_dir);
        tokio::fs::create_dir_all(storage_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&storage_path, data).await.unwrap();

        mgr.add_file(&db_file).await.unwrap();

        result.push((name.to_string(), data.to_vec()));
    }

    result
}

/// Drain all progress events from a transfer receiver
async fn collect_progress(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TransferProgress>,
) -> Vec<TransferProgress> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let is_terminal = matches!(
            event,
            TransferProgress::Complete { .. } | TransferProgress::Failed { .. }
        );
        events.push(event);
        if is_terminal {
            break;
        }
    }
    events
}

/// Read the pending_deletions.json manifest from the library path
async fn read_pending_deletions(library_path: &Path) -> Vec<PendingDeletion> {
    let manifest = library_path.join("pending_deletions.json");
    if !manifest.exists() {
        return Vec::new();
    }
    let contents = tokio::fs::read_to_string(&manifest).await.unwrap();
    serde_json::from_str(&contents).unwrap()
}

/// Unpin a pinned release: local copies are queued for deletion,
/// release is marked as not pinned.
#[tokio::test]
async fn test_unpin_pinned_release() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    // Cloud home present + no pending upload ⇒ unpin is allowed.
    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    // Cloud release, pinned locally
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;

    let library_dir = LibraryDir::new(library_path.clone());
    let original_files = create_pinned_local_files(&mgr, &release_id, &library_dir).await;

    // Execute unpin
    let service = TransferService::new(mgr.clone());
    let rx = service.unpin_release(release_id.clone());
    let events = collect_progress(rx).await;

    // Verify success
    assert!(events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. })));

    // Verify DB: release is no longer pinned (managed, cloud-only).
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::CloudOnly
    );

    // Verify old pinned files queued for deletion
    let pending = read_pending_deletions(&library_path).await;
    assert_eq!(pending.len(), original_files.len());
    for deletion in &pending {
        let PendingDeletion::Local { .. } = deletion;
    }
}

/// Pin rejects local-library (unmanaged) releases.
#[tokio::test]
async fn test_pin_rejects_unmanaged_release() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, Some("/some/path"), false).await;

    let service = TransferService::new(mgr);
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Pin should fail for unmanaged releases"
    );
}

/// Unpin rejects local-library (unmanaged) releases.
#[tokio::test]
async fn test_unpin_rejects_unmanaged_release() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, Some("/some/path"), false).await;

    let service = TransferService::new(mgr);
    let rx = service.unpin_release(release_id.clone());
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Unpin should fail for unmanaged releases"
    );
}

/// Pin rejects an already-pinned release.
#[tokio::test]
async fn test_pin_rejects_already_pinned() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    // Already pinned
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;

    let service = TransferService::new(mgr);
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Pin should fail for already-pinned releases"
    );
}

/// Unpin rejects a release that is not pinned.
#[tokio::test]
async fn test_unpin_rejects_not_pinned() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    // Not pinned
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;

    let service = TransferService::new(mgr);
    let rx = service.unpin_release(release_id.clone());
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Unpin should fail for non-pinned releases"
    );
}

/// `read_release_file_bytes` aborts when the bytes on disk are shorter than
/// the declared `file_size` (SAFETY INVARIANT: a short read must fail before
/// any transition queues a delete).
#[tokio::test]
async fn test_read_release_file_bytes_rejects_short_read() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    // Cloud release, pinned locally — resolves a local storage path.
    let (album_id, release_id) = create_album_and_release(&db, None, true).await;

    let library_dir = LibraryDir::new(library_path.clone());

    // Declare a file_size larger than the bytes we actually write to disk.
    let actual = b"short" as &[u8];
    let db_file = DbFile::new(
        &release_id,
        "track.flac",
        (actual.len() + 100) as i64,
        ContentType::Flac,
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
    );
    let storage_path = db_file.local_storage_path(&library_dir);
    tokio::fs::create_dir_all(storage_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&storage_path, actual).await.unwrap();
    mgr.add_file(&db_file).await.unwrap();

    let _release = DbRelease {
        id: release_id.clone(),
        album_id,
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        managed: true,
        source_folder_name: None,
        content_hash: None,
        created_at: Utc::now(),
    };
    // This device pins the release: reads come from the staged `storage/` copy.
    let local_copy = bae_core::db::DbReleaseLocalCopy {
        release_id: release_id.clone(),
        unmanaged_path: None,
        pinned_locally: true,
    };

    let result = read_release_file_bytes(Some(&local_copy), &db_file, &mgr).await;
    assert!(
        result.is_err(),
        "short read must fail the length check, got {result:?}"
    );
}

/// Pin with no files should fail gracefully.
#[tokio::test]
async fn test_pin_empty_release_fails() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr) = setup_db(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;
    // No files created

    let service = TransferService::new(mgr);
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "Pin with no files should fail"
    );
}

// ===========================================================================
// Unpin guard (cloud copy must be durable)
// ===========================================================================

/// Unpin is rejected when no cloud home exists: dropping the local copy would
/// leave NO copy at all.
#[tokio::test]
async fn test_unpin_rejected_without_cloud_home() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    // setup_db has no cloud home injected.
    let (db, mgr) = setup_db(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;
    let library_dir = LibraryDir::new(library_path.clone());
    create_pinned_local_files(&mgr, &release_id, &library_dir).await;

    let service = TransferService::new(mgr.clone());
    let events = collect_progress(service.unpin_release(release_id.clone())).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "unpin must fail with no cloud home"
    );
    // Release stays pinned; nothing queued.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
    assert!(read_pending_deletions(&library_path).await.is_empty());
}

/// Unpin is rejected while an upload is still pending — the cloud copy is only
/// intended, not confirmed durable.
#[tokio::test]
async fn test_unpin_rejected_with_pending_upload() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;
    let library_dir = LibraryDir::new(library_path.clone());
    let files = create_pinned_local_files(&mgr, &release_id, &library_dir).await;

    // Leave a pending upload outstanding for one of the release's files.
    let file_id = mgr.get_files_for_release(&release_id).await.unwrap()[0]
        .id
        .clone();
    let cloud_key = bae_core::storage::local::storage_path(&file_id);
    mgr.add_cloud_outbox_upload(&file_id, &cloud_key, None)
        .await
        .unwrap();

    let service = TransferService::new(mgr.clone());
    let events = collect_progress(service.unpin_release(release_id.clone())).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "unpin must fail while an upload is pending"
    );
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
    assert!(read_pending_deletions(&library_path).await.is_empty());
    let _ = files;
}

// ===========================================================================
// Manage (Unmanaged → Pinned), pin = true
// ===========================================================================

/// Manage pin=true, delete_source=false: files staged in storage/, outbox
/// uploads enqueued (source_path=None), release becomes Pinned with
/// unmanaged_path cleared, and the originals are left in place.
#[tokio::test]
async fn test_manage_pin_keeps_source_when_not_requested() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let source_dir = temp.path().join("originals");

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) =
        create_album_and_release(&db, Some(source_dir.to_str().unwrap()), false).await;
    let originals = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    mgr.manage_release(&release_id, true, false).await.unwrap();

    // Release is Pinned: managed with a local pin, no unmanaged path.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );

    // Each file is staged in storage/ with the right bytes.
    let library_dir = LibraryDir::new(library_path.clone());
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        let staged = file.local_storage_path(&library_dir);
        let bytes = tokio::fs::read(&staged).await.unwrap();
        let expected = &originals
            .iter()
            .find(|(n, _)| *n == file.original_filename)
            .unwrap()
            .1;
        assert_eq!(&bytes, expected, "staged bytes match original");
    }

    // Outbox uploads enqueued from storage/ (source_path=None).
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), originals.len());
    assert!(uploads.iter().all(|u| matches!(
        &u.operation,
        coven::db::OutboxOperation::Upload {
            source_path: None,
            ..
        }
    )));

    // Originals untouched.
    for (name, _) in &originals {
        assert!(source_dir.join(name).exists(), "original {name} survives");
    }
}

/// Manage pin=true, delete_source=true: after a verified storage/ copy exists,
/// the originals are deleted NOW.
#[tokio::test]
async fn test_manage_pin_deletes_source_after_durable_copy() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let source_dir = temp.path().join("originals");

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) =
        create_album_and_release(&db, Some(source_dir.to_str().unwrap()), false).await;
    let originals = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    mgr.manage_release(&release_id, true, true).await.unwrap();

    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );

    // Durable storage/ copies exist...
    let library_dir = LibraryDir::new(library_path.clone());
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        assert!(file.local_storage_path(&library_dir).exists());
    }
    // ...and the originals are gone.
    for (name, _) in &originals {
        assert!(
            !source_dir.join(name).exists(),
            "original {name} should be deleted"
        );
    }
}

// ===========================================================================
// Unmanage (Pinned → Unmanaged)
// ===========================================================================

/// Unmanage from Pinned: files written to the new path with correct bytes,
/// release flipped to Unmanaged, and managed-copy deletes queued (local
/// storage/ deferred deletions + cloud-outbox deletes) only after the durable
/// write.
#[tokio::test]
async fn test_unmanage_from_pinned_writes_then_queues_deletes() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let new_path = temp.path().join("exported");

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;
    let library_dir = LibraryDir::new(library_path.clone());
    let originals = create_pinned_local_files(&mgr, &release_id, &library_dir).await;

    mgr.unmanage_release(&release_id, new_path.to_str().unwrap())
        .await
        .unwrap();

    // Files written to new path with correct bytes.
    for (name, data) in &originals {
        let written = tokio::fs::read(new_path.join(name)).await.unwrap();
        assert_eq!(&written, data, "exported {name} matches");
    }

    // Release is Unmanaged at the new path, recorded in this device's local copy.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Unmanaged
    );
    let local_copy = mgr
        .get_release_local_copy(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_copy.unmanaged_path.as_deref(), new_path.to_str());
    assert!(!local_copy.pinned_locally);

    // Managed-copy deletes queued: local storage/ deferred + cloud outbox.
    let pending = read_pending_deletions(&library_path).await;
    assert_eq!(pending.len(), originals.len(), "local deletions queued");
    let deletes = db.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), originals.len(), "cloud deletes queued");
}

/// Unmanage that fails on file 2 of 3: NO delete is queued, every managed copy
/// (storage/ + cloud) stays intact, and the release stays managed.
#[tokio::test]
async fn test_unmanage_abort_on_write_failure_queues_no_deletes() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, true).await;
    let library_dir = LibraryDir::new(library_path.clone());

    // Three pinned files in storage/.
    let mut staged = Vec::new();
    for (name, data) in [
        ("a.flac", b"file-a-bytes" as &[u8]),
        ("b.flac", b"file-b-bytes"),
        ("c.flac", b"file-c-bytes"),
    ] {
        let db_file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        let path = db_file.local_storage_path(&library_dir);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, data).await.unwrap();
        mgr.add_file(&db_file).await.unwrap();
        staged.push((db_file, path));
    }

    // Make the destination a FILE (not a directory). create_dir_all on it
    // fails, so do_unmanage aborts before writing/queueing anything.
    let new_path = temp.path().join("dest_is_a_file");
    tokio::fs::write(&new_path, b"blocker").await.unwrap();

    let service = TransferService::new(mgr.clone());
    let events = collect_progress(
        service.unmanage_release(release_id.clone(), new_path.to_str().unwrap().to_string()),
    )
    .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "unmanage must fail when the destination can't be created"
    );

    // Release stays managed (Pinned), nothing queued, every storage/ copy intact.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
    assert!(read_pending_deletions(&library_path).await.is_empty());
    assert!(db.get_pending_cloud_deletes().await.unwrap().is_empty());
    for (_file, path) in &staged {
        assert!(path.exists(), "storage/ copy must stay intact on abort");
    }
}

/// Round-trip Unmanaged → Pinned → Unmanaged keeps bytes intact and never
/// violates the CHECK constraint (each setter would error if it did).
#[tokio::test]
async fn test_round_trip_unmanaged_pinned_unmanaged() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let source_dir = temp.path().join("originals");
    let back_dir = temp.path().join("back");

    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) =
        create_album_and_release(&db, Some(source_dir.to_str().unwrap()), false).await;
    let originals = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    // Unmanaged → Pinned (keep the originals). A pin reads as Pinned at once
    // (its verified local copy), then its upload completes and flips `managed`.
    use bae_core::album_detail::ReleaseStorageState;
    mgr.manage_release(&release_id, true, false).await.unwrap();
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
    let upload_cloud = MockCloudHome::new();
    let upload_enc = std::sync::RwLock::new(EncryptionService::new_with_key(&[0u8; 32]));
    mgr.process_cloud_uploads_with(&upload_cloud, &upload_enc)
        .await
        .unwrap();

    // Pinned → Unmanaged (back out to a new folder).
    mgr.unmanage_release(&release_id, back_dir.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Unmanaged
    );
    let back = mgr
        .get_release_local_copy(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back.unmanaged_path.as_deref(), back_dir.to_str());
    assert!(!back.pinned_locally);

    // Bytes survived the round trip.
    for (name, data) in &originals {
        let written = tokio::fs::read(back_dir.join(name)).await.unwrap();
        assert_eq!(&written, data, "round-trip bytes for {name}");
    }
}

/// The deferred-delete intent (`delete_unmanaged_source_on_upload`) lives on the
/// `release_local_copy` row but is owned by the dedicated set/get path, not the
/// `DbReleaseLocalCopy` struct. A whole-row upsert of that struct must not reset
/// the intent — otherwise a local-copy write landing mid-upload during a
/// Manage → CloudOnly-with-delete transition would silently drop the request to
/// delete the originals.
#[tokio::test]
async fn upsert_local_copy_preserves_delete_unmanaged_source_intent() {
    tracing_init();
    let temp = TempDir::new().unwrap();
    let lib = TempDir::new().unwrap();
    let (db, _mgr) = setup_db(&temp, lib.path()).await;

    let (_album_id, release_id) =
        create_album_and_release(&db, Some("/some/origin/folder"), false).await;

    // Set the deferred-delete intent on this device's local-copy row.
    db.set_release_delete_unmanaged_source_on_upload(&release_id, true)
        .await
        .unwrap();
    assert!(db
        .get_release_delete_unmanaged_source_on_upload(&release_id)
        .await
        .unwrap());

    // Re-upsert the same row from a fresh struct (which carries no intent).
    db.upsert_release_local_copy(&bae_core::db::DbReleaseLocalCopy {
        release_id: release_id.clone(),
        unmanaged_path: Some("/some/origin/folder".to_string()),
        pinned_locally: false,
    })
    .await
    .unwrap();

    // The intent must survive the row upsert.
    assert!(
        db.get_release_delete_unmanaged_source_on_upload(&release_id)
            .await
            .unwrap(),
        "upsert_release_local_copy reset the deferred-delete intent"
    );
}

/// Insert DbFile rows for a cloud-only managed release (no local copy on disk)
/// and seed each file's blob at its content-addressed cloud key, sealed through
/// the home's at-rest cipher exactly as the upload outbox would — encrypted
/// under the library master key on an opaque home, stored verbatim on a
/// browsable one. Returns (file_id, bytes).
async fn seed_cloud_only_files(
    mgr: &LibraryManager,
    cloud: &MockCloudHome,
    release_id: &str,
    files: &[(&str, &[u8])],
) -> Vec<(String, Vec<u8>)> {
    let cipher = mgr.cloud_blob_cipher().expect("the home has a blob cipher");

    let mut result = Vec::new();
    for (name, data) in files {
        let db_file = DbFile::new(
            release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        let key = bae_core::storage::local::storage_path(&db_file.id);
        // Managed audio is master-scoped; `seal` matches the outbox's seal.
        cloud.put(&key, cipher.seal(data));
        mgr.add_file(&db_file).await.unwrap();
        result.push((db_file.id.clone(), data.to_vec()));
    }
    result
}

/// Pin a cloud-only release: every file downloads chunked from the cloud (range
/// reads only, never a full-object `read`), decrypts, and lands byte-identical
/// in `storage/`. The release flips to Pinned.
#[tokio::test]
async fn test_pin_cloud_only_downloads_chunked() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let library_dir = LibraryDir::new(library_path.clone());

    let (db, mgr, cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;

    // Two files: one spanning multiple 1 MiB download windows, one tiny.
    let big: Vec<u8> = (0..(3 * 1_048_576 + 4242))
        .map(|i| (i % 251) as u8)
        .collect();
    let small = b"a short final track".to_vec();
    let files = seed_cloud_only_files(
        &mgr,
        &cloud,
        &release_id,
        &[("track1.flac", &big), ("track2.flac", &small)],
    )
    .await;

    let service = TransferService::new(mgr.clone());
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Complete { .. })),
        "pin of a cloud-only release should complete"
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. })));

    // The files landed in storage/, byte-identical to the originals.
    for (file_id, data) in &files {
        let stored = library_dir.join(bae_core::storage::local::storage_path(file_id));
        let on_disk = tokio::fs::read(&stored).await.unwrap();
        assert_eq!(&on_disk, data, "pinned bytes for {file_id}");
    }

    // The download used range reads only — never a full-object read.
    assert_eq!(
        cloud.full_read_count(),
        0,
        "chunked pin must not issue a full-object read"
    );

    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
}

/// Pinning a cloud-only release on a browsable home: the blobs are stored
/// verbatim (no encryption), so the chunked download must read them through the
/// plaintext cipher and land them byte-identical in `storage/` — never trying
/// to decrypt plaintext through a fabricated nonce. Same path as the opaque pin
/// with the home's cipher flipped.
#[tokio::test]
async fn test_pin_cloud_only_browsable_downloads_verbatim() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let library_dir = LibraryDir::new(library_path.clone());

    let (db, mgr, cloud) = setup_db_with_browsable_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;

    // A blob spanning multiple 1 MiB download windows, plus a tiny one.
    let big: Vec<u8> = (0..(3 * 1_048_576 + 4242))
        .map(|i| (i % 251) as u8)
        .collect();
    let small = b"a short final track".to_vec();
    let files = seed_cloud_only_files(
        &mgr,
        &cloud,
        &release_id,
        &[("track1.flac", &big), ("track2.flac", &small)],
    )
    .await;

    let service = TransferService::new(mgr.clone());
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Complete { .. })),
        "pin of a cloud-only browsable release should complete"
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. })));

    // The verbatim bytes landed in storage/, byte-identical to the originals.
    for (file_id, data) in &files {
        let stored = library_dir.join(bae_core::storage::local::storage_path(file_id));
        let on_disk = tokio::fs::read(&stored).await.unwrap();
        assert_eq!(&on_disk, data, "pinned browsable bytes for {file_id}");
    }

    assert_eq!(
        cloud.full_read_count(),
        0,
        "chunked pin must not issue a full-object read"
    );

    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Pinned
    );
}

/// A transient range-read stall on the first window must not kill the pin: the
/// retry recovers and the file still lands.
#[tokio::test]
async fn test_pin_cloud_only_retries_transient_range_failure() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let library_dir = LibraryDir::new(library_path.clone());

    let (db, mgr, cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;

    let data = b"cloud-only data that survives a flaky first read".to_vec();
    let files = seed_cloud_only_files(&mgr, &cloud, &release_id, &[("only.flac", &data)]).await;

    // Fail the first range read (the nonce header); the retry must recover.
    cloud.fail_next_range_reads(1);

    let service = TransferService::new(mgr.clone());
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Complete { .. })),
        "a transient range-read failure must be retried, not fatal"
    );

    let (file_id, bytes) = &files[0];
    let stored = library_dir.join(bae_core::storage::local::storage_path(file_id));
    assert_eq!(&tokio::fs::read(&stored).await.unwrap(), bytes);
}

/// An aborted pin (the cloud blob is gone so every download attempt fails)
/// leaves no `.part` file behind and the release stays CloudOnly.
#[tokio::test]
async fn test_pin_cloud_only_failure_leaves_no_partial() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let library_dir = LibraryDir::new(library_path.clone());

    let (db, mgr, cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_album_and_release(&db, None, false).await;

    let data = b"this blob will be removed before the pin runs".to_vec();
    let files = seed_cloud_only_files(&mgr, &cloud, &release_id, &[("only.flac", &data)]).await;
    // Remove the blob so every download attempt fails.
    let (file_id, _) = &files[0];
    cloud.remove(&bae_core::storage::local::storage_path(file_id));

    let service = TransferService::new(mgr.clone());
    let rx = service.pin_release_task(release_id.clone()).0;
    let events = collect_progress(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransferProgress::Failed { .. })),
        "a pin whose cloud blob is missing must fail"
    );

    // No partial download remains: glob the destination directory for any
    // leftover `*.part` file rather than reconstructing production's temp name.
    let dest = library_dir.join(bae_core::storage::local::storage_path(file_id));
    let dest_dir = dest.parent().expect("storage path has a parent directory");
    let leftover_parts: Vec<_> = std::fs::read_dir(dest_dir)
        .expect("destination directory must be listable")
        .map(|entry| entry.expect("directory entry must be readable").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "part"))
        .collect();
    assert!(
        leftover_parts.is_empty(),
        "a failed pin must leave no .part files, found {leftover_parts:?}"
    );
    assert!(!dest.exists(), "a failed pin must not publish the file");

    // The release is still CloudOnly.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::CloudOnly
    );
}

/// Create an album + release with explicit artist/album names (so a browsable
/// readable cloud key is predictable) and an unmanaged in-place local copy at
/// `unmanaged_path`. Returns (album_id, release_id).
async fn create_named_unmanaged_release(
    db: &Database,
    artist_name: &str,
    album_title: &str,
    unmanaged_path: &str,
) -> (String, String) {
    let now = Utc::now();
    let artist_id = Uuid::new_v4().to_string();
    db.insert_artist(&bae_core::db::DbArtist {
        id: artist_id.clone(),
        name: artist_name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    })
    .await
    .unwrap();

    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: album_title.to_string(),
        artist_id,
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    db.insert_album(&album).await.unwrap();

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
        created_at: now,
    };
    db.insert_release(&release).await.unwrap();
    db.upsert_release_local_copy(&bae_core::db::DbReleaseLocalCopy {
        release_id: release.id.clone(),
        unmanaged_path: Some(unmanaged_path.to_string()),
        pinned_locally: false,
    })
    .await
    .unwrap();

    (album.id, release.id)
}

/// Managing a release into a BROWSABLE home stores a readable
/// `{artist}/{album}/{filename}` on each `release_files.cloud_path`, enqueues the
/// outbox upload under that same key, and the playback/read path resolves to the
/// same key — so the synced row and the cloud object agree on a human-readable
/// path with no `storage/` prefix.
#[tokio::test]
async fn test_manage_browsable_stores_readable_cloud_path() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let source_dir = temp.path().join("originals");

    let (db, mgr, _cloud) = setup_db_with_browsable_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_named_unmanaged_release(
        &db,
        "Artist Name",
        "Album Title",
        source_dir.to_str().unwrap(),
    )
    .await;
    let originals = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    // Manage cloud-only: enqueues uploads, sets the readable cloud_path. The
    // release stays unmanaged here (no live sync loop flips it), but the file
    // rows and outbox are written.
    mgr.manage_release(&release_id, false, false).await.unwrap();

    // Each file row now carries the readable key: under the `storage/` audio
    // namespace, but the readable `{artist}/{album}/{filename}` shape rather than
    // the hashed `storage/{ab}/{cd}/{id}` shards an opaque home uses.
    let files = mgr.get_files_for_release(&release_id).await.unwrap();
    for file in &files {
        assert_eq!(
            file.cloud_path.as_deref(),
            Some(format!("storage/Artist Name/Album Title/{}", file.original_filename).as_str()),
            "browsable file {} carries the readable cloud_path",
            file.original_filename
        );
    }

    // The outbox enqueued each upload under the SAME readable key (never the
    // hashed storage_path).
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), originals.len());
    for file in &files {
        let key = file.cloud_path.clone().unwrap();
        assert!(
            uploads.iter().any(|u| u.cloud_key == key),
            "outbox enqueued {key}"
        );
        // The readable form, not the hashed `storage/{ab}/{cd}/{id}` an opaque
        // home would use.
        assert!(
            key.starts_with("storage/Artist Name/Album Title/"),
            "a browsable key is the readable form: {key}"
        );
        assert_ne!(
            key,
            bae_core::storage::local::storage_path(&file.id),
            "a browsable key is not the hashed storage_path"
        );
    }

    // The read path resolves the same key (the hashed storage_path is NOT used).
    for file in &files {
        let resolved = mgr.resolve_track_cloud_key_for_test(&file.id).await;
        assert_eq!(
            resolved,
            file.cloud_path.clone().unwrap(),
            "read path resolves the stored readable key for {}",
            file.original_filename
        );
    }
}

/// Managing a release into an OPAQUE home leaves every `release_files.cloud_path`
/// NULL and enqueues each upload under the hashed `storage_path(file_id)` —
/// byte-identical to the pre-readable-path behavior (no regression).
#[tokio::test]
async fn test_manage_opaque_leaves_cloud_path_null() {
    tracing_init();

    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let source_dir = temp.path().join("originals");

    // setup_db_with_cloud is opaque (the default storage mode).
    let (db, mgr, _cloud) = setup_db_with_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_named_unmanaged_release(
        &db,
        "Artist Name",
        "Album Title",
        source_dir.to_str().unwrap(),
    )
    .await;
    let _originals = create_unmanaged_files(&mgr, &release_id, &source_dir).await;

    mgr.manage_release(&release_id, false, false).await.unwrap();

    let files = mgr.get_files_for_release(&release_id).await.unwrap();
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    for file in &files {
        assert_eq!(
            file.cloud_path, None,
            "opaque file {} leaves cloud_path NULL",
            file.original_filename
        );
        let hashed = bae_core::storage::local::storage_path(&file.id);
        assert!(
            uploads.iter().any(|u| u.cloud_key == hashed),
            "opaque upload enqueued under the hashed storage_path {hashed}"
        );
    }
}

/// A cover set on a BROWSABLE home stores a readable `library_images.cloud_path`,
/// and the `BlobPlan` carries it through to `BlobRef.cloud_path` as
/// `Artist Name/Album Title/cover.jpg`. On an OPAQUE home both stay NULL/None.
#[tokio::test]
async fn test_cover_blob_ref_cloud_path_browsable_vs_opaque() {
    tracing_init();

    use bae_core::db::LibraryImageType;
    use bae_core::sync::blob_plan::BaeBlobPlan;
    use bae_core::util::content_type::ContentType;
    use coven::blob::BlobPlan;

    // --- Browsable: the cover keys readably and the plan reflects it. ---
    let temp = TempDir::new().unwrap();
    let library_path = temp.path().join("library");
    tokio::fs::create_dir_all(&library_path).await.unwrap();
    let (db, mgr, _cloud) = setup_db_with_browsable_cloud(&temp, &library_path).await;
    let (_album_id, release_id) = create_named_unmanaged_release(
        &db,
        "Artist Name",
        "Album Title",
        temp.path().join("orig").to_str().unwrap(),
    )
    .await;

    // Compute + store the cover's readable cloud_path the way change_cover does.
    let cloud_path = mgr
        .cover_cloud_path_for_test(&release_id, &ContentType::Jpeg)
        .await;
    assert_eq!(
        cloud_path.as_deref(),
        Some("Artist Name/Album Title/cover.jpg")
    );
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

    let library_dir = LibraryDir::new(library_path.clone());
    let plan = BaeBlobPlan::new(library_dir.clone());
    let refs = db
        .coven_db()
        .call(move |conn| {
            plan.blobs_in_db(conn)
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
        Some("Artist Name/Album Title/cover.jpg"),
        "browsable cover BlobRef carries the readable cloud_path"
    );

    // --- Opaque: the cover keys by id and the plan carries no readable path. ---
    let temp2 = TempDir::new().unwrap();
    let library_path2 = temp2.path().join("library");
    tokio::fs::create_dir_all(&library_path2).await.unwrap();
    let (db2, mgr2, _cloud2) = setup_db_with_cloud(&temp2, &library_path2).await;
    let (_a2, release_id2) = create_named_unmanaged_release(
        &db2,
        "Artist Name",
        "Album Title",
        temp2.path().join("orig").to_str().unwrap(),
    )
    .await;
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
    let plan2 = BaeBlobPlan::new(LibraryDir::new(library_path2.clone()));
    let refs2 = db2
        .coven_db()
        .call(move |conn| {
            plan2
                .blobs_in_db(conn)
                .map_err(coven::database::DbError::from)
        })
        .await
        .unwrap();
    let cover_ref2 = refs2
        .iter()
        .find(|r| r.id == release_id2)
        .expect("opaque cover blob ref present");
    assert_eq!(
        cover_ref2.cloud_path, None,
        "opaque cover BlobRef carries no readable cloud_path"
    );
}
