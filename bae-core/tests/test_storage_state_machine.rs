#![cfg(feature = "test-utils")]
//! Tests for the release storage state machine onto coven's owned-blob model:
//! make-Remote (Local → Remote, the drain flips the gate after the uploads land)
//! and make-Local (Remote → Local, materialize back through coven's cache then
//! retract + tombstone). The local-only windows live in `test_transfer.rs`.
//!
//! Storage is TWO states — Local (the user's own file in place, a coven
//! user-provided external ref) and Remote (a cloud blob fronted by coven's
//! cache). Pinned-ness is the ORTHOGONAL per-device coven-cache property: whether
//! coven keeps a Remote blob in `storage/pinned/` (offline) vs the evictable
//! `storage/cache/`. A `retain_pinned` make-Remote populates the pinned cache as
//! it drains, so the release becomes (Remote, pinned) only after the upload lands.
//!
//! coven owns the transitions (gate flip, source delete, materialize, retract,
//! tombstone) and the durable-copy-before-delete ordering; tests drive them via
//! coven's free functions (`make_remote` + the drain via
//! `process_cloud_uploads_with`, `make_local` via `make_local_for_test`). The
//! cross-device gate retract + asset keep/leak behavior is exercised in coven's
//! own gate tests (a `covers`/`artist_images` asset rides its subject's gate and
//! never keeps it alive).

mod support;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{Database, DbAlbum, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
use bae_core::encryption::EncryptionService;
use bae_core::library::{CancellationToken, LibraryManager};
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

/// This release's (storage state, pinned) facts on this device.
async fn storage(mgr: &LibraryManager, release_id: &str) -> (ReleaseStorageState, bool) {
    let s = mgr
        .find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap();
    (s.storage_state, s.pinned)
}

/// This device's raw `releases.remote` gate flag for a release.
async fn remote_flag(mgr: &LibraryManager, release_id: &str) -> bool {
    mgr.get_release_by_id(release_id)
        .await
        .unwrap()
        .unwrap()
        .remote
}

/// The full hashed cloud object key a release file's blob lives at on an opaque
/// home (`release_files/{ab}/{cd}/{id}`) — what the drain uploads under and a read
/// fetches from.
fn release_file_key(file_id: &str) -> String {
    coven::sync::cloud_storage::CloudSyncStorage::blob_key(
        coven::sync::cloud_storage::BlobPathScheme::Hashed,
        "release_files",
        file_id,
        None,
    )
    .unwrap()
}

/// Build a manager plus a `MockCloudHome`/encryption pair WITHOUT injecting them,
/// so a test can decide when the cloud home appears.
async fn setup_manager(
    tmp: &TempDir,
) -> (
    Database,
    LibraryManager,
    Arc<MockCloudHome>,
    EncryptionService,
) {
    let library_dir = LibraryDir::new(tmp.path());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let db_path = tmp.path().join("test.db");
    let db = Database::new_test(
        db_path.to_str().unwrap(),
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
    let cloud = Arc::new(MockCloudHome::new());
    let enc = EncryptionService::new_with_key(&[9u8; 32]);
    (db, mgr, cloud, enc)
}

/// Build a manager with a `MockCloudHome` + encryption injected, so the cloud
/// read/write paths resolve without a live SyncManager. `sync_ready` models the
/// running sync loop the manager's make-Remote gate requires.
async fn setup_with_cloud(
    tmp: &TempDir,
    sync_ready: bool,
) -> (
    Database,
    LibraryManager,
    Arc<MockCloudHome>,
    EncryptionService,
) {
    let (db, mut mgr, cloud, enc) = setup_manager(tmp).await;
    mgr.set_cloud_override(cloud.clone(), enc.clone());
    if sync_ready {
        mgr.set_force_sync_ready();
    }
    (db, mgr, cloud, enc)
}

/// Insert an artist + album + a Local release, write its originals under
/// `source_dir`, register the `DbFile` rows, and register each as a coven
/// user-provided external ref (the in-place files of a Local release). No uploads
/// queued — the test drives the transition itself. Returns (release_id, files).
async fn create_local_release(
    db: &Database,
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    let artist_id = "test-storage-artist";
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
    let release_id = release.id.clone();
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[], &[])
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
            now,
        );
        mgr.add_file(&db_file).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    db.register_release_external_refs_for_test(&release_id, &source_dir.to_string_lossy())
        .await
        .unwrap();
    (release_id, result)
}

/// A Remote (cloud-only, not pinned) release whose blobs sit in `cloud`: create it
/// Local, make-Remote via coven, and drain so the gate flips. Returns (release_id,
/// [(file_id, original_filename, plaintext)]).
async fn create_remote_cloud_only_release(
    db: &Database,
    mgr: &LibraryManager,
    cloud: &MockCloudHome,
    enc_svc: EncryptionService,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    let (release_id, _named) = create_local_release(db, mgr, source_dir, files).await;

    let mut captured = Vec::new();
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        let plaintext = files
            .iter()
            .find(|(n, _)| *n == file.original_filename)
            .map(|(_, d)| d.to_vec())
            .unwrap();
        captured.push((file.id.clone(), file.original_filename.clone(), plaintext));
    }

    mgr.make_remote_for_test(&release_id, false).await.unwrap();
    let enc = RwLock::new(enc_svc);
    let count = mgr.process_cloud_uploads_with(cloud, &enc).await.unwrap();
    assert_eq!(count, files.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "release is Remote (cloud-only) after the drain"
    );
    (release_id, captured)
}

// ---------------------------------------------------------------------------
// make-Remote with pin: Local before the drain, Remote + pinned after.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_remote_import_becomes_pinned_after_upload() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) =
        create_local_release(&db, &mgr, &source_dir, &[("track1.flac", b"track1-bytes")]).await;

    // Enqueue a retain-pinned make-Remote. Before the drain the gate hasn't
    // flipped and nothing is in the cache, so the release is Local + not pinned.
    mgr.make_remote_for_test(&release_id, true).await.unwrap();
    assert!(
        !remote_flag(&mgr, &release_id).await,
        "make-Remote lands remote=false until its upload drains"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false),
        "Local + not-pinned before the upload drains"
    );

    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // The drain flipped the gate AND the retain-pinned upload populated the pinned
    // cache, so the release reads Remote + pinned.
    assert!(remote_flag(&mgr, &release_id).await);
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true),
        "a drained retain-pinned make-Remote lands Remote + pinned"
    );
}

// ---------------------------------------------------------------------------
// Multiple releases: completing one doesn't flip another.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_releases_independent_completion() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;

    let (release_a, _a) =
        create_local_release(&db, &mgr, &tmp.path().join("a"), &[("track.flac", b"aaa")]).await;
    let (release_b, _b) =
        create_local_release(&db, &mgr, &tmp.path().join("b"), &[("track.flac", b"bbb")]).await;
    mgr.make_remote_for_test(&release_a, true).await.unwrap();
    mgr.make_remote_for_test(&release_b, true).await.unwrap();

    let enc = RwLock::new(enc_svc);

    // First drain: release_a completes and flips; the drain breaks to publish, so
    // release_b is left for the next pass.
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(
        remote_flag(&mgr, &release_a).await,
        "release_a flipped remote"
    );
    assert!(
        !remote_flag(&mgr, &release_b).await,
        "release_b is untouched while release_a completes"
    );

    // Second drain: release_b completes on its own.
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(
        remote_flag(&mgr, &release_b).await,
        "release_b flipped remote"
    );

    assert_eq!(
        storage(&mgr, &release_a).await,
        (ReleaseStorageState::Remote, true)
    );
    assert_eq!(
        storage(&mgr, &release_b).await,
        (ReleaseStorageState::Remote, true)
    );
}

// ---------------------------------------------------------------------------
// make-Remote requires a live upload pipeline.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_refused_when_sync_not_running() {
    let tmp = TempDir::new().unwrap();
    // Cloud home configured but the sync loop is NOT running.
    let (db, mgr, _cloud, _enc) = setup_with_cloud(&tmp, false).await;
    assert!(
        !mgr.is_sync_ready(),
        "precondition: the sync loop must not be running"
    );
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(
        &db,
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // The manager's make-Remote gate refuses up front when the pipeline is down —
    // nothing would ever drain the queue to flip the release Remote.
    let result = mgr.make_release_remote(&release_id, false).await;
    assert!(
        result.is_err(),
        "make-Remote must fail when the upload pipeline isn't running, got {result:?}"
    );
    assert!(
        db.coven_db()
            .get_pending_cloud_uploads()
            .await
            .unwrap()
            .is_empty(),
        "no upload may be enqueued when the pipeline can't drain it"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
}

// ---------------------------------------------------------------------------
// make-Remote (cloud-only): upload from the originals; coven deletes them once
// the last upload lands. The drained blob is NOT pinned.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_cloud_only_uploads_from_source_then_completes() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &db,
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    mgr.make_remote_for_test(&release_id, false).await.unwrap();

    // Outbox uploads read from the originals (source_path = Some).
    let uploads = db.coven_db().get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), files.len());
    assert!(uploads.iter().all(|u| matches!(
        &u.operation,
        coven::db::OutboxOperation::Upload {
            source_path: Some(_),
            ..
        }
    )));

    // Still Local until the uploads finish.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );

    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, files.len());

    // coven flipped it Remote (cloud-only, NOT pinned) and deleted the originals
    // once the last upload landed.
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

/// make-Remote whose upload FAILS: coven never flips, the originals survive, the
/// release stays Local, and the upload is still pending.
#[tokio::test]
async fn test_manage_upload_failure_keeps_source() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud, enc_svc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &db,
        &mgr,
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.make_remote_for_test(&release_id, false).await.unwrap();

    let failing = MockCloudHome::failing();
    let enc = RwLock::new(enc_svc);
    let count = mgr
        .process_cloud_uploads_with(&failing, &enc)
        .await
        .unwrap();
    assert_eq!(count, 0, "no upload should succeed");

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

/// make-Remote refuses a truncated source: coven verifies every external source
/// (exists + length == registered size) up front, so a short original aborts with
/// nothing enqueued and the source intact.
#[tokio::test]
async fn test_manage_truncated_source_aborts_before_enqueue() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud, _enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &db,
        &mgr,
        &source_dir,
        &[("a.flac", b"full-length-bytes"), ("b.flac", b"second-file")],
    )
    .await;

    // Corrupt one original: fewer bytes on disk than its registered size.
    tokio::fs::write(source_dir.join("a.flac"), b"x")
        .await
        .unwrap();

    let result = mgr.make_remote_for_test(&release_id, false).await;
    assert!(result.is_err(), "truncated source must abort make-Remote");

    assert!(
        db.coven_db()
            .get_pending_cloud_uploads()
            .await
            .unwrap()
            .is_empty(),
        "no upload may be enqueued when a source is truncated"
    );
    for (name, _) in &files {
        assert!(source_dir.join(name).exists(), "source {name} must survive");
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
}

// ---------------------------------------------------------------------------
// make-Local from Remote: materialize each file back through coven's cache,
// register the external refs, then tombstone the cloud blobs.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_unmanage_from_remote_reads_through_cache_then_queues_deletes() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
        &source_dir,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    mgr.make_local_for_test(
        &release_id,
        new_path.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    // Files read back through coven's cache (cloud on a miss) and written to the
    // new path with the right bytes.
    for (_file_id, name, plaintext) in &files {
        let written = tokio::fs::read(new_path.join(name)).await.unwrap();
        assert_eq!(&written, plaintext, "exported {name} matches");
    }

    // Release is Local; coven registered each file's external ref at the new path,
    // and queued a cloud delete per blob after the durable write.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
    for (file_id, name, _) in &files {
        let path = mgr.file_local_path(file_id).await.unwrap().unwrap();
        assert_eq!(
            path,
            new_path.join(name),
            "external ref points at the new path"
        );
    }
    let deletes = db.coven_db().get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), files.len());
}

/// make-Local of a Remote release whose cloud blob is missing: a hard error (the
/// cache-miss read 404s), nothing written, no delete queued, release stays Remote.
#[tokio::test]
async fn test_unmanage_from_remote_missing_blob_is_hard_error() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud, enc) = setup_with_cloud(&tmp, true).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        &mgr,
        cloud.as_ref(),
        enc,
        &source_dir,
        &[("x.flac", b"present"), ("y.flac", b"missing")],
    )
    .await;

    // Remove the SECOND file's blob from the cloud so its cache-miss read 404s.
    cloud.remove(&release_file_key(&files[1].0));

    let new_path = tmp.path().join("exported");
    let result = mgr
        .make_local_for_test(
            &release_id,
            new_path.to_str().unwrap(),
            &CancellationToken::new(),
        )
        .await;
    assert!(result.is_err(), "missing blob must be a hard error");

    assert!(db
        .coven_db()
        .get_pending_cloud_deletes()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
    assert!(!new_path.join(&files[1].1).exists());
}

// ---------------------------------------------------------------------------
// Cloud-home reactivity: a release's available storage actions are baked in from
// whether a cloud home exists. Adding one must re-emit every album so cached UI
// details refresh without a restart.
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
    let (db, mut mgr, cloud, enc) = setup_manager(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) =
        create_local_release(&db, &mgr, &source_dir, &[("a.flac", b"a")]).await;

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

    let mut rx = mgr.subscribe_events();
    mgr.emit_all_albums_updated().await;
    let before = collect_library_events(&mut rx, std::time::Duration::from_millis(200)).await;
    assert!(
        actions_for(&before, &release_id).is_empty(),
        "no cloud home → no storage actions"
    );

    mgr.set_cloud_override(cloud, enc);
    mgr.emit_all_albums_updated().await;
    let after = collect_library_events(&mut rx, std::time::Duration::from_millis(200)).await;
    assert!(
        actions_for(&after, &release_id).contains(&ReleaseStorageAction::MakeRemote),
        "cloud home present → make-Remote available"
    );
}
