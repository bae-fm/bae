#![cfg(feature = "test-utils")]
//! Tests for the release storage state machine: the cloud-dependent transition
//! windows (Manage → CloudOnly via the upload observer, Unmanage from CloudOnly
//! via a cloud download). The local-only windows live in `test_transfer.rs`.
//!
//! SAFETY INVARIANT: a durable verified copy must exist at the destination
//! before any delete is queued. The CloudOnly transitions exercise the upload
//! observer's deferred-delete (originals removed only after the last upload
//! lands) and the cloud-read durability check (a missing/short blob aborts
//! before any delete).

mod support;

use bae_core::db::{DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::encryption::EncryptionService;
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::sync::{Arc, RwLock};
use support::MockCloudHome;
use tempfile::TempDir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup(tmp: &TempDir) -> LibraryManager {
    let library_dir = LibraryDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let db_path = tmp.path().join("test.db");
    let db = bae_core::db::Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    LibraryManager::new(
        db,
        library_dir,
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    )
}

/// This device's storage state for a release, derived from the storage summary
/// the manager resolves from the two device-local stores.
async fn storage_state(
    mgr: &LibraryManager,
    release_id: &str,
) -> bae_core::album_detail::ReleaseStorageState {
    mgr.find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap()
        .storage_state
}

/// This device's raw `releases.managed` flag for a release.
async fn managed_flag(mgr: &LibraryManager, release_id: &str) -> bool {
    mgr.get_release_by_id(release_id)
        .await
        .unwrap()
        .unwrap()
        .managed
}

/// Create a managed (pinned) album+release+files, write files to storage/.
async fn create_pinned_release(mgr: &LibraryManager, filenames: &[&str]) -> String {
    let now = Utc::now();

    // Insert a test artist for the album FK
    let artist_id = "test-storage-artist";
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
        title: "Test Album".to_string(),
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
        // A pin lands unmanaged with a verified local copy and uploads queued;
        // the upload observer flips `managed` true once they finish. Tests that
        // drain the uploads exercise that real flip.
        managed: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let release_id = release.id.clone();

    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
        .await
        .unwrap();

    let mut files = Vec::new();
    for filename in filenames {
        let file = DbFile::new(
            &release_id,
            filename,
            1000,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        let storage_path = mgr.local_storage_path_for_file(&file);
        std::fs::create_dir_all(storage_path.parent().unwrap()).unwrap();
        std::fs::write(&storage_path, format!("data-{filename}").as_bytes()).unwrap();

        let cloud_key = bae_core::storage::local::storage_path(&file.id);
        mgr.add_file(&file).await.unwrap();
        mgr.add_cloud_outbox_upload(&file.id, &cloud_key, None)
            .await
            .unwrap();
        files.push(file);
    }

    // Pin this device's cache copies (managed stays false until the uploads
    // land). The `storage/` copies are already staged above.
    mgr.set_pinned_cache(&release_id, &files).await.unwrap();

    release_id
}

// ---------------------------------------------------------------------------
// Import with cloud: Pinned stays Pinned after upload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_managed_import_stays_pinned_after_upload() {
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;

    let release_id = create_pinned_release(&mgr, &["track1.flac"]).await;

    // Lands unmanaged (its upload hasn't finished) yet already reads Pinned —
    // this device holds the verified local copy.
    assert!(
        !managed_flag(&mgr, &release_id).await,
        "a pin lands managed=false until its upload completes"
    );
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::Pinned
    );

    let cloud = MockCloudHome::new();
    let enc = RwLock::new(EncryptionService::new_with_key(&[0u8; 32]));
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);

    // The upload completed: the observer flipped it managed while keeping the pin.
    assert!(
        managed_flag(&mgr, &release_id).await,
        "the upload observer flips a pin to managed once its uploads finish"
    );
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::Pinned
    );
}

// ---------------------------------------------------------------------------
// Multiple releases: completing one doesn't affect another
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_releases_independent_completion() {
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;

    let release_a = create_pinned_release(&mgr, &["track.flac"]).await;
    let release_b = create_pinned_release(&mgr, &["track.flac"]).await;

    let cloud = MockCloudHome::new();
    let enc = RwLock::new(EncryptionService::new_with_key(&[0u8; 32]));

    // First drain: release_a's upload completes and flips it managed; the observer
    // breaks the drain to publish, so release_b is left for the next pass —
    // completing one release does not flip another.
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);
    assert!(
        managed_flag(&mgr, &release_a).await,
        "release_a flipped managed",
    );
    assert!(
        !managed_flag(&mgr, &release_b).await,
        "release_b is untouched while release_a completes",
    );

    // Second drain: release_b completes and flips on its own.
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);
    assert!(
        managed_flag(&mgr, &release_b).await,
        "release_b flipped managed",
    );

    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_a).await,
        ReleaseStorageState::Pinned
    );
    assert_eq!(
        storage_state(&mgr, &release_b).await,
        ReleaseStorageState::Pinned
    );
}

/// Build a manager plus a `MockCloudHome`/encryption pair WITHOUT injecting them,
/// so a test can decide when the cloud home appears — mirroring a connect that
/// flips `get_cloud_home()` from `None` to `Some`. `set_cloud_override` (used by
/// `setup_with_cloud` and the reactivity test) seeds the cloud read/write paths
/// so they resolve without a live SyncManager.
async fn setup_manager(tmp: &TempDir) -> (LibraryManager, Arc<MockCloudHome>, EncryptionService) {
    let library_dir = LibraryDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let db_path = tmp.path().join("test.db");
    let db = bae_core::db::Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let mgr = LibraryManager::new(
        db,
        library_dir,
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );
    let cloud = Arc::new(MockCloudHome::new());
    let enc = EncryptionService::new_with_key(&[9u8; 32]);
    (mgr, cloud, enc)
}

/// Build a manager with a `MockCloudHome` + encryption injected, so the cloud
/// read/write paths resolve without a live SyncManager. The same encryption
/// service is returned so a test can seed encrypted blobs the manager will
/// decrypt.
async fn setup_with_cloud(
    tmp: &TempDir,
    sync_ready: bool,
) -> (LibraryManager, Arc<MockCloudHome>, EncryptionService) {
    let (mut mgr, cloud, enc) = setup_manager(tmp).await;
    mgr.set_cloud_override(cloud.clone(), enc.clone());
    // Tests that drive the upload pipeline by hand via
    // `process_cloud_uploads_with` pass `sync_ready: true` to model a running
    // sync loop, which the CloudOnly manage gate requires. The refusal test
    // passes `false` to leave the gate reading the real (absent) sync loop.
    if sync_ready {
        mgr.set_force_sync_ready();
    }
    (mgr, cloud, enc)
}

/// Insert a test artist + album + an Unmanaged release, write its originals to
/// `source_dir`, and register the DbFile rows. Returns (release_id, files).
async fn create_unmanaged_release(
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    let artist_id = "test-cloudonly-artist";
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
        title: "CloudOnly Album".to_string(),
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
        managed: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let release_id = release.id.clone();
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
        .await
        .unwrap();
    // Unmanaged in place: mark managed=false and record this device's
    // unmanaged source at the source directory.
    mgr.unmanage_release_storage(&release_id, &source_dir.to_string_lossy())
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        tokio::fs::write(source_dir.join(name), data).await.unwrap();
        let db_file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        mgr.add_file(&db_file).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    (release_id, result)
}

// ---------------------------------------------------------------------------
// Manage → CloudOnly requires a live upload pipeline.
//
// CloudOnly keeps no local managed copy: the release only becomes managed once
// `ReleaseUploadObserver` confirms the last upload landed, and that observer
// fires only from inside the running sync loop. With a cloud home configured
// but the sync loop not running, the uploads never drain, the observer never
// fires, and the release would stay Unmanaged forever — so the transition must
// refuse up front instead of silently succeeding.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_cloud_only_refused_when_sync_not_running() {
    let tmp = TempDir::new().unwrap();
    // Cloud home configured but the sync loop is NOT running.
    let (mgr, _cloud, _enc) = setup_with_cloud(&tmp, false).await;
    assert!(
        !mgr.is_sync_ready(),
        "precondition: the sync loop must not be running"
    );
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_unmanaged_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // CloudOnly manage with the pipeline down must error, not return Ok: nothing
    // would ever drain the queue to flip the release managed.
    let result = mgr.manage_release(&release_id, false, false).await;
    assert!(
        result.is_err(),
        "manage → CloudOnly must fail when the upload pipeline isn't running, got {result:?}"
    );

    // Nothing was enqueued and the release stays Unmanaged.
    assert!(
        mgr.get_pending_cloud_uploads().await.unwrap().is_empty(),
        "no upload may be enqueued when the pipeline can't drain it"
    );
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Unmanaged
    );
}

// ---------------------------------------------------------------------------
// Manage → CloudOnly (pin = false): upload from the originals; the observer
// clears unmanaged_path once the last upload lands.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_cloud_only_uploads_from_source_then_observer_clears_path() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_unmanaged_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // Manage to CloudOnly, no source deletion.
    mgr.manage_release(&release_id, false, false).await.unwrap();

    // Outbox uploads read from the originals (source_path = Some).
    let uploads = mgr.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), files.len());
    assert!(uploads.iter().all(|u| matches!(
        &u.operation,
        coven::db::OutboxOperation::Upload {
            source_path: Some(_),
            ..
        }
    )));

    // Still Unmanaged until the uploads finish.
    use bae_core::album_detail::ReleaseStorageState;
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::Unmanaged
    );

    // Drive the uploads (this fires the real ReleaseUploadObserver).
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, files.len());

    // Observer flipped it to managed → CloudOnly. Source NOT deleted.
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        ReleaseStorageState::CloudOnly
    );
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} kept");
    }
}

#[tokio::test]
async fn test_manage_cloud_only_deferred_delete_removes_source_after_last_upload() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_unmanaged_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"defer-a"), ("b.flac", b"defer-bb")],
    )
    .await;

    // Manage to CloudOnly WITH source deletion requested (deferred).
    mgr.manage_release(&release_id, false, true).await.unwrap();

    // Deferred: originals still present right after enqueue.
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} not yet gone");
    }

    // Drive uploads → observer fires on the last one, deletes the originals.
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, files.len());

    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::CloudOnly
    );
    for (name, _) in &files {
        assert!(
            !source_dir.join(name).exists(),
            "source {name} should be deleted after last upload"
        );
    }
}

/// Manage → CloudOnly with delete_source requested, but the upload FAILS: the
/// observer never fires, so the originals survive (no eager delete) and the
/// release stays Unmanaged with the intent still pending.
#[tokio::test]
async fn test_manage_cloud_only_upload_failure_keeps_source() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_unmanaged_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.manage_release(&release_id, false, true).await.unwrap();

    // A cloud whose writes all fail: process_uploads stops at the first one and
    // never notifies the observer.
    let failing = MockCloudHome::failing();
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(&failing, &enc)
        .await
        .unwrap();
    assert_eq!(count, 0, "no upload should succeed");

    // Source intact; release still Unmanaged; the upload is still pending.
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} must survive");
    }
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::Unmanaged
    );
    assert_ne!(
        mgr.count_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        0
    );
}

/// Manage → CloudOnly refuses a truncated source. The original is the upload
/// source and the delete is deferred to the observer, so a source whose on-disk
/// bytes are shorter than the recorded file_size must abort BEFORE anything is
/// enqueued — otherwise short bytes would upload and the only full copy would
/// then be deleted. All-or-nothing: nothing enqueued, source intact, still
/// Unmanaged.
#[tokio::test]
async fn test_manage_cloud_only_truncated_source_aborts_before_enqueue() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, _enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_unmanaged_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"full-length-bytes"), ("b.flac", b"second-file")],
    )
    .await;

    // Corrupt one original: fewer bytes on disk than its recorded file_size.
    tokio::fs::write(source_dir.join("a.flac"), b"x")
        .await
        .unwrap();

    let result = mgr.manage_release(&release_id, false, true).await;
    assert!(result.is_err(), "truncated source must abort manage");

    assert!(
        mgr.get_pending_cloud_uploads().await.unwrap().is_empty(),
        "no upload may be enqueued when a source is truncated"
    );
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} must survive");
    }
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::Unmanaged,
        "release must stay Unmanaged after an aborted manage"
    );
}

// ---------------------------------------------------------------------------
// Unmanage from CloudOnly: download + decrypt each file, write to the new
// path, then queue cloud deletes — only after a verified durable write.
// ---------------------------------------------------------------------------

/// Insert a CloudOnly release (unmanaged_path = None, not pinned) and register
/// its DbFile rows. Returns (release_id, [(file_id, original_filename,
/// plaintext)]). The caller seeds the cloud blobs.
async fn create_cloud_only_release(
    mgr: &LibraryManager,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    let now = Utc::now();
    let artist_id = "cloudonly-dl-artist";
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
        title: "CloudOnly DL".to_string(),
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
        managed: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let release_id = release.id.clone();
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
        .await
        .unwrap();

    let mut result = Vec::new();
    for (name, data) in files {
        let db_file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        let id = db_file.id.clone();
        mgr.add_file(&db_file).await.unwrap();
        result.push((id, name.to_string(), data.to_vec()));
    }
    (release_id, result)
}

#[tokio::test]
async fn test_unmanage_from_cloud_only_downloads_then_queues_deletes() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let (release_id, files) = create_cloud_only_release(
        &mgr,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    // Seed encrypted blobs at the content-addressed cloud keys, encrypted with
    // the library master key — that's what the unmanage download decrypts with.
    for (file_id, _name, plaintext) in &files {
        let key = bae_core::storage::local::storage_path(file_id);
        cloud.put(&key, enc.encrypt(plaintext));
    }

    let new_path = tmp.path().join("exported");
    mgr.unmanage_release(&release_id, new_path.to_str().unwrap())
        .await
        .unwrap();

    // Files downloaded + decrypted to the new path with the right bytes.
    for (_file_id, name, plaintext) in &files {
        let written = tokio::fs::read(new_path.join(name)).await.unwrap();
        assert_eq!(&written, plaintext, "downloaded {name} matches");
    }

    // Release is Unmanaged at the new path, and cloud deletes were queued for
    // every blob (after the durable write).
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::Unmanaged
    );
    let source = mgr
        .get_unmanaged_source(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Some(source.path.as_str()), new_path.to_str());
    let deletes = mgr.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), files.len());
}

/// Unmanage of a CloudOnly release whose cloud blob is missing: a hard error,
/// nothing written for that file, no delete queued, release stays managed.
#[tokio::test]
async fn test_unmanage_from_cloud_only_missing_blob_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let (release_id, files) =
        create_cloud_only_release(&mgr, &[("x.flac", b"present"), ("y.flac", b"missing")]).await;

    // Seed only the FIRST file's blob (encrypted with the library master key);
    // the second is absent.
    let key0 = bae_core::storage::local::storage_path(&files[0].0);
    cloud.put(&key0, enc.encrypt(&files[0].2));

    let new_path = tmp.path().join("exported");
    let result = mgr
        .unmanage_release(&release_id, new_path.to_str().unwrap())
        .await;
    assert!(result.is_err(), "missing blob must be a hard error");

    // No delete was queued; the release stays CloudOnly (managed).
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::CloudOnly
    );

    // The missing file was never written to the new path.
    assert!(!new_path.join(&files[1].1).exists());
}

/// Unmanage of a CloudOnly release whose cloud blob decrypts to fewer bytes
/// than `file_size`: the length check aborts, no delete queued.
#[tokio::test]
async fn test_unmanage_from_cloud_only_short_blob_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let (release_id, files) =
        create_cloud_only_release(&mgr, &[("x.flac", b"the-full-length-bytes")]).await;

    // Seed a blob (encrypted with the library master key) that decrypts to FEWER
    // bytes than the declared file_size, so the length check is what aborts.
    let key = bae_core::storage::local::storage_path(&files[0].0);
    cloud.put(&key, enc.encrypt(b"short"));

    let new_path = tmp.path().join("exported");
    let result = mgr
        .unmanage_release(&release_id, new_path.to_str().unwrap())
        .await;
    assert!(result.is_err(), "short blob must fail the length check");

    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
    assert_eq!(
        storage_state(&mgr, &release_id).await,
        bae_core::album_detail::ReleaseStorageState::CloudOnly
    );
}

// ---------------------------------------------------------------------------
// Cloud-home reactivity: a release's available storage actions are baked into
// the resolved release from whether a cloud home exists. Adding/removing one
// must re-emit every album so cached UI details refresh without a restart.
// ---------------------------------------------------------------------------

async fn collect_library_events(
    rx: &mut tokio::sync::broadcast::Receiver<bae_core::library::LibraryEvent>,
    timeout: std::time::Duration,
) -> Vec<bae_core::library::LibraryEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(timeout, rx.recv()).await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn emit_all_albums_updated_flips_storage_actions_on_cloud_home_transition() {
    use bae_core::album_detail::ReleaseStorageAction;
    use bae_core::library::LibraryEvent;

    let tmp = TempDir::new().unwrap();
    // No cloud home yet.
    let (mut mgr, cloud, enc) = setup_manager(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) =
        create_unmanaged_release(&mgr, &source_dir, &[("a.flac", b"a")]).await;

    // Pull this release's storage actions out of the next AlbumUpdated event.
    let actions_for = |events: &[LibraryEvent], rel: &str| -> Vec<ReleaseStorageAction> {
        events
            .iter()
            .find_map(|e| match e {
                LibraryEvent::AlbumUpdated { album } => album
                    .releases
                    .iter()
                    .find(|r| r.summary.id == rel)
                    .map(|r| r.summary.storage_actions.clone()),
                _ => None,
            })
            .expect("expected an AlbumUpdated carrying the release")
    };

    // Without a cloud home, re-emitting yields no storage actions.
    let mut rx = mgr.subscribe_events();
    mgr.emit_all_albums_updated().await;
    let before = collect_library_events(&mut rx, std::time::Duration::from_millis(200)).await;
    assert!(
        actions_for(&before, &release_id).is_empty(),
        "no cloud home → no storage actions"
    );

    // Adding a cloud home re-emits with Manage now available — the reactivity fix.
    mgr.set_cloud_override(cloud, enc);
    mgr.emit_all_albums_updated().await;
    let after = collect_library_events(&mut rx, std::time::Duration::from_millis(200)).await;
    assert!(
        actions_for(&after, &release_id).contains(&ReleaseStorageAction::Manage),
        "cloud home present → Manage available"
    );
}

// ---------------------------------------------------------------------------
// Transition coverage: the sabotage check that encapsulation is load-bearing.
//
// Every storage transition lands the release in one of the THREE valid states
// (Unmanaged / Pinned / CloudOnly). The invalid tuple (managed = false, no
// unmanaged source, not pinned — issue #105's "nowhere to read") is
// unrepresentable: no transition produces it. We drive every transition and
// assert the resolved state is always valid, AND that the raw stores never hold
// the forbidden tuple. Uses public + test-utils transition entry points only.
// ---------------------------------------------------------------------------

/// Assert the release's raw stores never form the invalid tuple, and return its
/// resolved state (always one of the three valid variants).
async fn assert_valid_state(
    mgr: &LibraryManager,
    db: &bae_core::db::Database,
    release_id: &str,
) -> bae_core::album_detail::ReleaseStorageState {
    use bae_core::album_detail::ReleaseStorageState;
    let managed = mgr
        .get_release_by_id(release_id)
        .await
        .unwrap()
        .unwrap()
        .managed;
    let has_source = db.get_unmanaged_source(release_id).await.unwrap().is_some();
    let files = mgr.get_files_for_release(release_id).await.unwrap();
    let cache = db.get_release_file_cache(release_id).await.unwrap();
    let all_pinned =
        bae_core::album_detail::all_files_pinned(files.iter().map(|f| f.id.as_str()), &cache);

    // The forbidden tuple must never occur: a release the device can't read.
    assert!(
        !(!managed && !has_source && !all_pinned),
        "invalid storage tuple for {release_id}: managed=false, no source, not pinned \
         (issue #105 — nowhere to read)"
    );

    let state = mgr
        .find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap()
        .storage_state;
    assert!(
        matches!(
            state,
            ReleaseStorageState::Unmanaged
                | ReleaseStorageState::Pinned
                | ReleaseStorageState::CloudOnly
        ),
        "resolved state must be one of the three valid variants, got {state:?}"
    );
    state
}

/// Insert an album + release + one file, returning the release id. `managed`
/// sets the initial shared flag.
async fn insert_bare_release(mgr: &LibraryManager, managed: bool) -> String {
    let now = Utc::now();
    let artist_id = format!("tc-artist-{}", Uuid::new_v4());
    mgr.insert_artist(&bae_core::db::DbArtist {
        id: artist_id.clone(),
        name: "Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    })
    .await
    .unwrap();
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Album".to_string(),
        artist_id,
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
        managed,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
        .await
        .unwrap();
    let file = DbFile::new(
        &release.id,
        "track.flac",
        10,
        ContentType::Flac,
        Uuid::new_v4().to_string(),
        now,
    );
    mgr.add_file(&file).await.unwrap();
    release.id
}

#[tokio::test]
async fn every_transition_lands_in_a_valid_state() {
    use bae_core::album_detail::ReleaseStorageState::*;

    let tmp = TempDir::new().unwrap();
    let library_dir = LibraryDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let db = bae_core::db::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let mgr = LibraryManager::new(
        db.clone(),
        library_dir,
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );

    // Import-unmanaged: managed=false + an unmanaged source. A cloud-only import
    // lands in this same valid Unmanaged tuple (it records an unmanaged source
    // and queues uploads); its end-to-end behavior through the real
    // `ImportService` — playable in place until the upload flips it to
    // CloudOnly — is covered in `test_pending_upload_source.rs`.
    {
        let rel = insert_bare_release(&mgr, false).await;
        db.test_set_unmanaged_source(&rel, "/src").await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Unmanaged);
    }

    // Import-pin: a pinned cache, no source.
    {
        let rel = insert_bare_release(&mgr, false).await;
        db.test_pin_all_files(&rel).await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Pinned);
    }

    // Manage → pinned (observer flip): set pinned cache (drops any source) then
    // mark managed-pinned.
    {
        let rel = insert_bare_release(&mgr, false).await;
        db.test_set_unmanaged_source(&rel, "/src").await.unwrap();
        let files = mgr.get_files_for_release(&rel).await.unwrap();
        mgr.set_pinned_cache(&rel, &files).await.unwrap(); // pin drops the source
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Pinned);
        db.test_mark_managed_pinned(&rel).await.unwrap(); // observer flip
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Pinned);
    }

    // Manage → cloud-only (observer flip): an unmanaged source, then the managed
    // flip drops it.
    {
        let rel = insert_bare_release(&mgr, false).await;
        db.test_set_unmanaged_source(&rel, "/src").await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Unmanaged);
        db.test_mark_managed_cloud_only(&rel).await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, CloudOnly);
    }

    // Pin (CloudOnly → Pinned) then Unpin (→ CloudOnly): a managed release.
    {
        let rel = insert_bare_release(&mgr, true).await;
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, CloudOnly);
        let files = mgr.get_files_for_release(&rel).await.unwrap();
        mgr.set_pinned_cache(&rel, &files).await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Pinned);
        mgr.unpin_release_locally(&rel).await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, CloudOnly);
    }

    // Unmanage (managed → Unmanaged): drops cache, sets source, managed=false.
    {
        let rel = insert_bare_release(&mgr, true).await;
        let files = mgr.get_files_for_release(&rel).await.unwrap();
        mgr.set_pinned_cache(&rel, &files).await.unwrap();
        db.test_mark_managed_pinned(&rel).await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Pinned);
        mgr.unmanage_release_storage(&rel, "/out").await.unwrap();
        assert_eq!(assert_valid_state(&mgr, &db, &rel).await, Unmanaged);
    }
}
