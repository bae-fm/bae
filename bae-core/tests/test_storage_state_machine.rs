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
//! tombstone) and the durable-copy-before-delete ordering; tests drive them
//! through the manager's coven seams (`coven_make_remote` + the upload drain via
//! `drain_uploads_expecting_work`, `coven_make_local`). The
//! cross-device gate retract + asset keep/leak behavior is exercised in coven's
//! own gate tests (a `covers`/`artist_images` asset rides its subject's gate and
//! never keeps it alive).

use bae_test_support as support;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{
    Database, DbAlbum, DbFile, DbRelease, Pressing, SortDirection, StorageFilter,
    StorageSortCriterion, StorageSortField,
};
use bae_core::library::{AppServices, CancellationToken, LibraryManager, StorageProjectionValue};
use bae_core::sync::CloudCipher;
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use coven::EncryptionService;
use coven::InMemoryCloudHome;
use coven::StoreDir;
use std::sync::Arc;
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

/// Build a manager plus a `InMemoryCloudHome`/encryption pair WITHOUT injecting them,
/// so a test can decide when the cloud home appears.
async fn setup_manager(
    tmp: &TempDir,
) -> (
    Database,
    LibraryManager,
    Arc<InMemoryCloudHome>,
    EncryptionService,
) {
    let library_dir = StoreDir::new(tmp.path());
    let config_handle = support::test_config(&library_dir);
    let db_path = tmp.path().join("test.db");
    let db = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let mgr = LibraryManager::new(
        db.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let cloud = Arc::new(InMemoryCloudHome::new());
    let enc = EncryptionService::from_key([9u8; 32]);
    (db, mgr, cloud, enc)
}

/// Build a manager with a connected `SyncManager` over an injected
/// `InMemoryCloudHome`, sealing blobs under `enc`, and no sync loop behind it —
/// these tests drive the upload drain themselves and assert what each pass
/// moved, which only holds while nothing else drains the queue.
async fn setup_with_cloud(tmp: &TempDir) -> (Database, LibraryManager, Arc<InMemoryCloudHome>) {
    let (db, mgr, cloud, enc) = setup_manager(tmp).await;
    mgr.connect_test_cloud_home_caller_driven(cloud.clone(), CloudCipher::Encrypted(enc))
        .await
        .unwrap();
    (db, mgr, cloud)
}

/// Insert an artist + album + a Local release, write its originals under
/// `source_dir`, register the `DbFile` rows, and register each as a coven
/// user-provided external ref (the in-place files of a Local release). No uploads
/// queued — the test drives the transition itself. Returns (release_id, files).
async fn create_local_release(
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    // Shared across every release this helper seeds, so only the first call
    // inserts it.
    let artist_id = bae_test_support::test_uuid("test-storage-artist");
    if mgr.get_artist_by_id(&artist_id).await.unwrap().is_none() {
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
    }
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
        metadata_provenance: Some(bae_core::import::MetadataProvenance::FileTags),
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let release_id = release.id.clone();
    mgr.insert_album_with_release_and_tracks(&album, &release, &[], &[])
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        let path = source_dir.join(name);
        tokio::fs::write(&path, data).await.unwrap();
        let db_file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            uuid::Uuid::new_v4().to_string(),
            now,
        );
        mgr.add_external_file_for_test(&db_file, &path)
            .await
            .unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    (release_id, result)
}

/// A Remote (cloud-only, not pinned) release whose blobs sit in `cloud`: create it
/// Local, make-Remote via coven, and drain so the gate flips. Returns (release_id,
/// [(file_id, original_filename, plaintext)]).
async fn create_remote_cloud_only_release(
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    let (release_id, _named) = create_local_release(mgr, source_dir, files).await;

    let mut captured = Vec::new();
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        let plaintext = files
            .iter()
            .find(|(n, _)| *n == file.original_filename)
            .map(|(_, d)| d.to_vec())
            .unwrap();
        captured.push((file.id.clone(), file.original_filename.clone(), plaintext));
    }

    mgr.coven_make_remote(&release_id, false).await.unwrap();
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, files.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "release is Remote (cloud-only) after the drain"
    );
    (release_id, captured)
}

// ---------------------------------------------------------------------------
// Multiple releases: completing one doesn't flip another.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_releases_independent_completion() {
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, _cloud) = setup_with_cloud(&tmp).await;

    let (release_a, _a) =
        create_local_release(&mgr, &tmp.path().join("a"), &[("track.flac", b"aaa")]).await;
    let (release_b, _b) =
        create_local_release(&mgr, &tmp.path().join("b"), &[("track.flac", b"bbb")]).await;
    mgr.coven_make_remote(&release_a, true).await.unwrap();
    mgr.coven_make_remote(&release_b, true).await.unwrap();

    // First drain: release_a completes and flips; the drain breaks to publish, so
    // release_b is left for the next pass.
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
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
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
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
    // No cloud home connected, so the sync loop is NOT running.
    let (db, mgr, _cloud, _enc) = setup_manager(&tmp).await;
    assert!(
        !mgr.is_sync_ready(),
        "precondition: no cloud home / sync loop not running"
    );
    assert!(
        !mgr.has_cloud_home(),
        "precondition: no cloud home connected"
    );
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // The manager's make-Remote gate refuses up front when the pipeline is down —
    // nothing would ever drain the queue to flip the release Remote.
    let result = mgr
        .make_releases_remote(std::slice::from_ref(&release_id), false)
        .await
        .expect("the batch reports a per-release refusal");
    let bae_core::library::MakeReleasesRemoteOutcome::Partial {
        receipt: None,
        failure,
    } = result
    else {
        panic!("make-Remote must refuse the release without a receipt");
    };
    assert_eq!(failure.release_ids, vec![release_id.clone()]);
    assert!(
        db.queued_upload_count_for_test().await.unwrap() == 0,
        "no upload may be enqueued when the pipeline can't drain it"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
}

// ---------------------------------------------------------------------------
// make-Remote (cloud-only): bae's storage summary is Local while the uploads are
// queued and Remote (NOT pinned) once the drain flips the gate.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn make_remote_summary_is_local_until_drain_then_remote_not_pinned() {
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, _cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    mgr.coven_make_remote(&release_id, false).await.unwrap();

    // Still Local until the uploads finish.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );

    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, files.len());

    // The drain flipped the gate: Remote, cloud-only (not pinned).
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
}

/// bae's storage summary reflects a FAILED make-Remote: coven never flips the
/// gate, so the summary stays Local and the upload stays pending (retriable) in
/// the count the UI reads. The upload-retry semantics are coven's.
#[tokio::test]
async fn upload_failure_leaves_summary_local_and_upload_pending() {
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.coven_make_remote(&release_id, false).await.unwrap();

    // The connect bootstrap already wrote through the home; arm the failure only
    // now, so the upload drain fails every blob.
    cloud.arm_write_failures();
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, 0, "no upload should succeed");

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

/// The upload drain refuses a truncated source while sealing its spool. The
/// durable queue retains the failed work for retry, the Local gate stays closed,
/// and the user's source files remain untouched.
#[tokio::test]
async fn truncated_source_keeps_root_local_and_failed_work_queued() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"full-length-bytes"), ("b.flac", b"second-file")],
    )
    .await;

    // Corrupt one original: fewer bytes on disk than its registered size.
    tokio::fs::write(source_dir.join("a.flac"), b"x")
        .await
        .unwrap();

    mgr.coven_make_remote(&release_id, false).await.unwrap();
    assert_eq!(
        mgr.drain_uploads_expecting_work().await.unwrap(),
        1,
        "the intact sibling uploads and remains reusable for retry"
    );

    assert_eq!(
        db.queued_upload_count_for_test().await.unwrap(),
        files.len(),
        "the failed root remains queued for an explicit retry"
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
// make-Local from Remote: bae's summary reads Local, each file's external ref
// resolves to the new path, and a cloud delete is queued per blob.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn make_local_updates_summary_external_refs_and_queues_deletes() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &mgr,
        &source_dir,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    mgr.coven_make_local(
        &release_id,
        new_path.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    // Release is Local; each file's external ref resolves to the new path, and a
    // cloud delete is queued per blob.
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
    assert_eq!(
        db.queued_delete_count_for_test().await.unwrap(),
        files.len()
    );
}

/// Remove one release file's cloud object from the mock home.
///
/// coven keys an exact object by its locator hash — `release_files/opaque/<hash>`
/// — so a test cannot name the object from the blob id. Read the blob through
/// coven once, which records the slot that read touched, remove exactly that
/// slot, then drop the cache copy the read just populated so the next read has
/// to go back to the (now empty) cloud.
async fn remove_cloud_blob(mgr: &LibraryManager, cloud: &InMemoryCloudHome, file_id: &str) {
    let blob = mgr
        .release_blob_ref_for_test(file_id)
        .await
        .expect("the release_files row");
    cloud.clear_exact_reads();
    mgr.materialize_release_blob_for_test(file_id)
        .await
        .expect("the blob is readable before it is removed");
    let slots = cloud.exact_reads();
    assert_eq!(slots.len(), 1, "one exact read for one blob");
    cloud.remove_exact_object(&slots[0]);
    mgr.evict_blob_for_test(&blob)
        .await
        .expect("drop the cache copy the probe read populated");
}

/// A failed make-Local (a missing cloud blob 404s on the cache-miss read) leaves
/// bae's state consistent: the manager surfaces the error, no cloud delete is
/// queued, and the summary stays Remote. The read-404 and make-Local rollback are
/// coven's; this asserts the bae seam + summary after the failure.
#[tokio::test]
async fn make_local_missing_blob_fails_leaving_summary_remote_and_no_deletes() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &mgr,
        &source_dir,
        &[("x.flac", b"present"), ("y.flac", b"missing")],
    )
    .await;

    // Remove the SECOND file's blob from the cloud so its cache-miss read 404s.
    // `files` carries `(file_id, original_filename, plaintext)`.
    remove_cloud_blob(&mgr, &cloud, &files[1].0).await;

    let new_path = tmp.path().join("exported");
    let result = mgr
        .coven_make_local(
            &release_id,
            new_path.to_str().unwrap(),
            &CancellationToken::new(),
        )
        .await;
    assert!(result.is_err(), "missing blob must be a hard error");

    assert_eq!(db.queued_delete_count_for_test().await.unwrap(), 0);
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
}

// ---------------------------------------------------------------------------
// Cloud-home reactivity: the storage value joins the database row with the
// current sync state, so connecting a home delivers the changed actions.
// ---------------------------------------------------------------------------

async fn next_storage_release(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<
        Result<StorageProjectionValue, bae_core::library::LibraryError>,
    >,
    release_id: &str,
) -> bae_core::album_detail::ReleaseSummary {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage subscription remains open")
            .expect("storage projection resolves");
        if let Some(row) = value
            .page
            .rows
            .into_iter()
            .find(|row| row.release.id == release_id)
        {
            return row.release;
        }
    }
}

#[tokio::test]
async fn storage_subscription_delivers_actions_on_cloud_home_transition() {
    use bae_core::album_detail::ReleaseStorageAction;

    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud, enc) = setup_manager(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(&mgr, &source_dir, &[("a.flac", b"a")]).await;

    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let mut values = services.subscribe_storage_values(
        &tokio::runtime::Handle::current(),
        StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        StorageFilter::All,
        0,
        50,
    );
    let before = next_storage_release(&mut values, &release_id).await;
    assert!(
        before.storage_actions.is_empty(),
        "no cloud home → no storage actions"
    );

    mgr.connect_test_cloud_home(cloud, CloudCipher::Encrypted(enc))
        .await
        .unwrap();
    let after = next_storage_release(&mut values, &release_id).await;
    assert!(
        after
            .storage_actions
            .contains(&ReleaseStorageAction::MakeRemote),
        "cloud home present → make-Remote available"
    );
}
