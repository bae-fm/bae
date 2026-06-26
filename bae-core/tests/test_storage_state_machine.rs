#![cfg(feature = "test-utils")]
//! Tests for the release storage state machine: the cloud-dependent transition
//! windows (Manage → Remote via the upload observer, Unmanage from Remote via
//! a cloud read). The local-only windows live in `test_transfer.rs`.
//!
//! Storage is TWO states — Local (a local file the user owns, in place) and
//! Remote (a cloud blob fronted by coven's cache). Pinned-ness is the ORTHOGONAL
//! per-device coven-cache property (`pinned: bool`): whether coven keeps a remote
//! blob in `storage/pinned/` (offline) vs the evictable `storage/cache/`. A blob
//! only becomes pinned once it is in coven's cache, which a `retain_pinned` upload
//! populates as it drains — so a pin-intent manage lands Local with its source
//! in place and an upload queued, and becomes Remote+pinned only after that upload
//! drains.
//!
//! SAFETY INVARIANT: a durable verified copy must exist at the destination before
//! any delete is queued. The Remote transitions exercise the upload observer's
//! deferred-delete (originals removed only after the last upload lands) and the
//! cloud-read durability check (a missing/short blob aborts before any delete).

mod support;

use bae_core::album_detail::ReleaseStorageState;
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

/// This release's storage facts on this device: its storage state (Local /
/// Remote, from `releases.remote`) and the orthogonal `pinned` cache property
/// (whether coven holds its blob in `storage/pinned/`). `find_release_storage_summary`
/// resolves both; it needs the release to have at least one file row, which every
/// test here creates.
async fn storage(mgr: &LibraryManager, release_id: &str) -> (ReleaseStorageState, bool) {
    let s = mgr
        .find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap();
    (s.storage_state, s.pinned)
}

/// This device's raw `releases.remote` flag for a release.
async fn remote_flag(mgr: &LibraryManager, release_id: &str) -> bool {
    mgr.get_release_by_id(release_id)
        .await
        .unwrap()
        .unwrap()
        .remote
}

/// Insert an artist + album + a Local release, write its source files under
/// `source_dir`, register the `DbFile` rows, and queue each file's cloud upload
/// carrying `retain_pinned`. The release starts `remote = false` with its source
/// recorded — exactly the state a pin-intent (retain_pinned=true) or cloud-only
/// (retain_pinned=false) manage produces before its uploads drain. Returns
/// (release_id, [(filename, bytes)]).
///
/// Draining these uploads (`process_cloud_uploads_with`) flips the release remote
/// and — when `retain_pinned` is true — populates coven's pinned cache from the
/// plaintext, so the release then reads `(Remote, true)`; with `retain_pinned`
/// false it reads `(Remote, false)`.
async fn create_local_with_uploads(
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    filenames: &[&str],
    retain_pinned: bool,
) -> (String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();

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
        // A manage (pin or cloud-only) lands Local with its source in place
        // and uploads queued; the upload observer flips `remote` true once they
        // finish. Tests that drain the uploads exercise that real flip.
        remote: false,
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
    mgr.set_release_local_path(&release_id, &source_dir.to_string_lossy())
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for filename in filenames {
        let data = format!("data-{filename}").into_bytes();
        let source_path = source_dir.join(filename);
        tokio::fs::write(&source_path, &data).await.unwrap();

        let file = DbFile::new(
            &release_id,
            filename,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
        );
        let cloud_key = bae_core::storage::local::storage_path(&file.id);
        mgr.add_file(&file).await.unwrap();
        mgr.add_cloud_outbox_upload(
            &file.id,
            &cloud_key,
            Some(&source_path.to_string_lossy()),
            retain_pinned,
        )
        .await
        .unwrap();
        result.push((filename.to_string(), data));
    }

    (release_id, result)
}

// ---------------------------------------------------------------------------
// Manage with pin: Local before the drain, Remote + pinned after.
//
// The pinned LABEL now requires coven's cache to be populated, which happens as
// the `retain_pinned` upload drains — so a pin-intent manage is NOT pinned before
// the upload lands. Before the drain the release is `(Local, false)`; after,
// `(Remote, true)`. (Old model read "Pinned" immediately off a staged local copy;
// that copy is gone — remote bytes live only in coven's cache.)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_remote_import_becomes_pinned_after_upload() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");

    let (release_id, _files) =
        create_local_with_uploads(&mgr, &source_dir, &["track1.flac"], true).await;

    // Before the drain the blob isn't in coven's cache yet: the release is still
    // Local (its upload hasn't finished) and NOT pinned.
    assert!(
        !remote_flag(&mgr, &release_id).await,
        "a pin-intent manage lands remote=false until its upload completes"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false),
        "a pin-intent manage is Local + not-pinned before the upload drains"
    );

    let cloud = MockCloudHome::new();
    let enc = RwLock::new(enc_svc);
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);

    // The upload completed: the observer flipped it remote, AND the retain-pinned
    // upload populated coven's pinned cache — so it now reads Remote + pinned.
    assert!(
        remote_flag(&mgr, &release_id).await,
        "the upload observer flips the release to remote once its uploads finish"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true),
        "a drained retain-pinned upload lands Remote + pinned"
    );
}

// ---------------------------------------------------------------------------
// Multiple releases: completing one doesn't affect another
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_releases_independent_completion() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, enc_svc) = setup_with_cloud(&tmp, true).await;

    let (release_a, _a) =
        create_local_with_uploads(&mgr, &tmp.path().join("a"), &["track.flac"], true).await;
    let (release_b, _b) =
        create_local_with_uploads(&mgr, &tmp.path().join("b"), &["track.flac"], true).await;

    let cloud = MockCloudHome::new();
    let enc = RwLock::new(enc_svc);

    // First drain: release_a's upload completes and flips it remote; the observer
    // breaks the drain to publish, so release_b is left for the next pass —
    // completing one release does not flip another.
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);
    assert!(
        remote_flag(&mgr, &release_a).await,
        "release_a flipped remote",
    );
    assert!(
        !remote_flag(&mgr, &release_b).await,
        "release_b is untouched while release_a completes",
    );

    // Second drain: release_b completes and flips on its own.
    let count = mgr.process_cloud_uploads_with(&cloud, &enc).await.unwrap();
    assert_eq!(count, 1);
    assert!(
        remote_flag(&mgr, &release_b).await,
        "release_b flipped remote",
    );

    // Both drained as retain-pinned: each is Remote + pinned.
    assert_eq!(
        storage(&mgr, &release_a).await,
        (ReleaseStorageState::Remote, true)
    );
    assert_eq!(
        storage(&mgr, &release_b).await,
        (ReleaseStorageState::Remote, true)
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
    // sync loop, which the Remote manage gate requires. The refusal test passes
    // `false` to leave the gate reading the real (absent) sync loop.
    if sync_ready {
        mgr.set_force_sync_ready();
    }
    (mgr, cloud, enc)
}

/// Insert a test artist + album + a Local release, write its originals to
/// `source_dir`, and register the DbFile rows (no uploads queued — the test drives
/// `make_release_remote` itself). Returns (release_id, files).
async fn create_local_release(
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
        remote: false,
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
    // Local in place: record this device's source folder.
    mgr.set_release_local_path(&release_id, &source_dir.to_string_lossy())
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
// Manage → Remote requires a live upload pipeline.
//
// A remote release keeps no local remote copy: the release only becomes remote
// once `ReleaseUploadObserver` confirms the last upload landed, and that observer
// fires only from inside the running sync loop. With a cloud home configured but
// the sync loop not running, the uploads never drain, the observer never fires,
// and the release would stay Local forever — so the transition must refuse up
// front instead of silently succeeding.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_refused_when_sync_not_running() {
    let tmp = TempDir::new().unwrap();
    // Cloud home configured but the sync loop is NOT running.
    let (mgr, _cloud, _enc) = setup_with_cloud(&tmp, false).await;
    assert!(
        !mgr.is_sync_ready(),
        "precondition: the sync loop must not be running"
    );
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // Manage with the pipeline down must error, not return Ok: nothing would ever
    // drain the queue to flip the release remote.
    let result = mgr.make_release_remote(&release_id, false).await;
    assert!(
        result.is_err(),
        "manage must fail when the upload pipeline isn't running, got {result:?}"
    );

    // Nothing was enqueued and the release stays Local (not pinned).
    assert!(
        mgr.get_pending_cloud_uploads().await.unwrap().is_empty(),
        "no upload may be enqueued when the pipeline can't drain it"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
}

// ---------------------------------------------------------------------------
// Manage → Remote (pin = false): upload from the originals; the observer drops
// the local-source row and deletes the originals once the last upload lands.
// Cloud-only: the drained blob is NOT pinned.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_cloud_only_uploads_from_source_then_observer_completes() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // Manage cloud-only (pin = false).
    mgr.make_release_remote(&release_id, false).await.unwrap();

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

    // Still Local (not pinned) until the uploads finish.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );

    // Drive the uploads (this fires the real ReleaseUploadObserver).
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, files.len());

    // Observer flipped it to Remote; cloud-only, so NOT pinned. The in-place
    // source files were deleted once the last upload landed.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
    for (name, _) in &files {
        assert!(
            !source_dir.join(name).exists(),
            "source {name} deleted after the last upload landed"
        );
    }
}

/// Manage → Remote, but the upload FAILS: the observer never fires, so the
/// originals survive (no eager delete), the release stays Local (not pinned),
/// and its upload is still pending.
#[tokio::test]
async fn test_manage_upload_failure_keeps_source() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.make_release_remote(&release_id, false).await.unwrap();

    // A cloud whose writes all fail: drain_uploads records each failure and keeps
    // draining, so none of the release's uploads succeed.
    let failing = MockCloudHome::failing();
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(&failing, &enc)
        .await
        .unwrap();
    assert_eq!(count, 0, "no upload should succeed");

    // Source intact; release still Local (not pinned); the upload is still
    // pending.
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} must survive");
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
    assert_ne!(
        mgr.count_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        0
    );
}

/// Manage refuses a truncated source. The original is the upload source and the
/// delete is deferred to the observer, so a source whose on-disk bytes are shorter
/// than the recorded file_size must abort BEFORE anything is enqueued — otherwise
/// short bytes would upload and the only full copy would then be deleted.
/// All-or-nothing: nothing enqueued, source intact, still Local.
#[tokio::test]
async fn test_manage_truncated_source_aborts_before_enqueue() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud, _enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"full-length-bytes"), ("b.flac", b"second-file")],
    )
    .await;

    // Corrupt one original: fewer bytes on disk than its recorded file_size.
    tokio::fs::write(source_dir.join("a.flac"), b"x")
        .await
        .unwrap();

    let result = mgr.make_release_remote(&release_id, false).await;
    assert!(result.is_err(), "truncated source must abort manage");

    assert!(
        mgr.get_pending_cloud_uploads().await.unwrap().is_empty(),
        "no upload may be enqueued when a source is truncated"
    );
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} must survive");
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false),
        "release must stay Local after an aborted manage"
    );
}

// ---------------------------------------------------------------------------
// Unmanage from Remote: read each file through coven's cache (cloud on a miss),
// write it to the new path, then queue cloud deletes — only after a verified
// durable write.
// ---------------------------------------------------------------------------

/// Get a Remote (cloud-only, not pinned) release whose blobs sit in the
/// `MockCloudHome`: insert it Local, queue cloud-only uploads, and drain them.
/// The observer flips it Remote and the blobs land in the home, so a later
/// unmanage/pin reads them back through coven's cache. Returns (release_id,
/// [(file_id, original_filename, plaintext)]).
async fn create_remote_cloud_only_release(
    mgr: &LibraryManager,
    cloud: &MockCloudHome,
    enc_svc: EncryptionService,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    let (release_id, _named) = create_local_release(mgr, source_dir, files).await;

    // Capture each file's id before draining (the file rows persist; the source
    // files get deleted by the observer).
    let mut result = Vec::new();
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        let plaintext = files
            .iter()
            .find(|(n, _)| *n == file.original_filename)
            .map(|(_, d)| d.to_vec())
            .unwrap();
        result.push((file.id.clone(), file.original_filename.clone(), plaintext));
    }

    mgr.make_release_remote(&release_id, false).await.unwrap();
    let enc = RwLock::new(enc_svc);
    let count = mgr.process_cloud_uploads_with(cloud, &enc).await.unwrap();
    assert_eq!(count, files.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "release is Remote (cloud-only) after the drain"
    );

    (release_id, result)
}

#[tokio::test]
async fn test_unmanage_from_remote_reads_through_cache_then_queues_deletes() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc,
        &source_dir,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    mgr.make_release_local(&release_id, new_path.to_str().unwrap())
        .await
        .unwrap();

    // Files read back through coven's cache (cloud on a miss) and written to the
    // new path with the right bytes.
    for (_file_id, name, plaintext) in &files {
        let written = tokio::fs::read(new_path.join(name)).await.unwrap();
        assert_eq!(&written, plaintext, "exported {name} matches");
    }

    // Release is Local at the new path, and cloud deletes were queued for
    // every blob (after the durable write).
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
    let source = mgr
        .get_release_local_source(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source.path.as_str(), new_path.to_str().unwrap());
    let deletes = mgr.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), files.len());
}

/// Unmanage of a Remote release whose cloud blob is missing: a hard error,
/// nothing written for that file, no delete queued, release stays Remote.
#[tokio::test]
async fn test_unmanage_from_remote_missing_blob_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc,
        &source_dir,
        &[("x.flac", b"present"), ("y.flac", b"missing")],
    )
    .await;

    // Remove the SECOND file's blob from the cloud so its cache-miss read 404s.
    cloud.remove(&bae_core::storage::local::storage_path(&files[1].0));

    let new_path = tmp.path().join("exported");
    let result = mgr
        .make_release_local(&release_id, new_path.to_str().unwrap())
        .await;
    assert!(result.is_err(), "missing blob must be a hard error");

    // No delete was queued; the release stays Remote (cloud-only).
    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );

    // The missing file was never written to the new path.
    assert!(!new_path.join(&files[1].1).exists());
}

/// Unmanage of a Remote release whose cloud blob decrypts to fewer bytes than
/// `file_size`: the length check aborts, no delete queued, release stays Remote.
#[tokio::test]
async fn test_unmanage_from_remote_short_blob_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &mgr,
        cloud.as_ref(),
        enc.clone(),
        &source_dir,
        &[("x.flac", b"the-full-length-bytes")],
    )
    .await;

    // Overwrite the cloud blob with one that decrypts to FEWER bytes than the
    // declared file_size, so the length check is what aborts. The blob is sealed
    // with the library master key, exactly as the upload sealed the original.
    let key = bae_core::storage::local::storage_path(&files[0].0);
    cloud.put(&key, enc.encrypt(b"short"));

    let new_path = tmp.path().join("exported");
    let result = mgr
        .make_release_local(&release_id, new_path.to_str().unwrap())
        .await;
    assert!(result.is_err(), "short blob must fail the length check");

    assert!(mgr.get_pending_cloud_deletes().await.unwrap().is_empty());
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
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
    let (release_id, _files) = create_local_release(&mgr, &source_dir, &[("a.flac", b"a")]).await;

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
        actions_for(&after, &release_id).contains(&ReleaseStorageAction::MakeRemote),
        "cloud home present → Manage available"
    );
}
