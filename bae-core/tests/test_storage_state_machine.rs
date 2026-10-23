#![cfg(feature = "test-utils")]
//! Release rows always sync. These tests exercise the manager's atomic file
//! attachment, durable upload queue, verified local copies, and pin state.
//! A release becomes Remote when attached, including while uploads are pending.
//! Pins independently keep its audio available offline; covers remain shared.

use bae_test_support as support;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{
    Database, DbAlbum, DbFile, DbRelease, SortDirection, StorageFilter, StorageSortCriterion,
    StorageSortField,
};
use bae_core::library::{
    AppServices, CancellationToken, LibraryManager, StorageBrowseSubscription, StorageBrowseView,
};
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use coven::MemoryStorage;
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

async fn setup_manager(tmp: &TempDir) -> (Database, LibraryManager, Arc<MemoryStorage>) {
    let (manager, database, storage) =
        support::storage_library(tmp.path(), bae_core::providers::Providers::offline()).await;
    (database, manager, storage)
}

async fn setup_with_cloud(tmp: &TempDir) -> (Database, LibraryManager, Arc<MemoryStorage>) {
    let (database, manager, storage) = setup_manager(tmp).await;
    support::connect_storage(&manager, &storage).await;
    (database, manager, storage)
}

/// Insert an artist + album + a Local release, write its originals under
/// `source_dir`, register the `DbFile` rows and bae's device-local paths. No uploads
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
        year: Some(2024),
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_core::pressing::Pressing::blank(),
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let release_id = release.id.clone();
    let credit = bae_core::db::DbAlbumArtist::new_test(&album.id, &artist_id);
    mgr.insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
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

/// A Remote (cloud-only, not pinned) release whose files sit in `cloud`: create it
/// Local, attach its files, and finish the uploads. Returns (release_id,
/// [(file_id, original_filename, plaintext)]).
async fn create_remote_cloud_only_release(
    db: &Database,
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

    mgr.upload_release(&release_id, false).await.unwrap();
    mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(db.queued_upload_count_for_test().await.unwrap(), 0);
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "release is Remote (cloud-only) once the transition is accepted"
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
    mgr.upload_release(&release_a, true).await.unwrap();
    mgr.upload_release(&release_b, true).await.unwrap();

    assert_eq!(
        mgr.count_pending_uploads_for_release(&release_a)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        mgr.count_pending_uploads_for_release(&release_b)
            .await
            .unwrap(),
        1
    );
    assert_eq!(mgr.drain_uploads_expecting_work().await.unwrap(), 2);
    assert_eq!(
        mgr.count_pending_uploads_for_release(&release_a)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        mgr.count_pending_uploads_for_release(&release_b)
            .await
            .unwrap(),
        0
    );

    let mut downloads = mgr.subscribe_download_queue();
    let settled = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        downloads.wait_for(|queue| {
            queue
                .ops
                .iter()
                .all(|op| matches!(op.state, bae_core::library::DownloadState::Failed { .. }))
        }),
    )
    .await
    .expect("both offline copies finish")
    .unwrap();
    assert!(settled.ops.is_empty(), "offline copy failed: {settled:?}");
    drop(settled);

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
// upload requires configured cloud storage.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_refused_without_cloud_storage() {
    let tmp = TempDir::new().unwrap();
    // No cloud home connected, so the sync loop is NOT running.
    let (db, mgr, _cloud) = setup_manager(&tmp).await;
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

    // No configured destination: refuse before attaching any files.
    let result = mgr
        .upload_releases(std::slice::from_ref(&release_id))
        .await
        .expect("the batch reports a per-release refusal");
    let bae_core::library::UploadReleasesOutcome::Partial {
        receipt: None,
        failure,
    } = result
    else {
        panic!("upload must refuse the release without a receipt");
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
// A release stays Local while its uploads are queued: rows name a file only
// once it is stored, so the release turns Remote when the last upload lands.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn attached_release_turns_remote_once_its_uploads_finish() {
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, _cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    mgr.upload_release(&release_id, false).await.unwrap();

    // Queued uploads leave every row without a stored file.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );

    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, files.len());

    // Each completed upload names its file, and the pin state is unchanged.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
}

/// Upload failure leaves the release Local and its files queued for retry.
#[tokio::test]
async fn upload_failure_keeps_attachment_and_pending_work() {
    let tmp = TempDir::new().unwrap();
    let (_db, mgr, cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(
        &mgr,
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.upload_release(&release_id, false).await.unwrap();

    // The connect bootstrap already wrote through the home; arm the failure only
    // now, so the upload drain fails every file.
    cloud.set_online(false);
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

/// Attachment refuses an original changed since import. Nothing is queued,
/// and the source files remain untouched.
#[tokio::test]
async fn truncated_source_refuses_attachment_without_queuing_work() {
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

    let error = mgr
        .upload_release(&release_id, false)
        .await
        .expect_err("a source changed since import");
    assert!(!error.to_string().is_empty());
    assert_eq!(db.queued_upload_count_for_test().await.unwrap(), 0);
    for (name, _) in files {
        assert!(source_dir.join(name).exists());
    }
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
}

// ---------------------------------------------------------------------------
// audio detachment from Remote: bae's summary reads Local and each file's device-local
// path names its new copy. The cloud objects the release stops naming are
// coven's to retire through accepted reclaim.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detaching_audio_updates_summary_and_original_paths() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, _cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        &mgr,
        &source_dir,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    mgr.detach_release_audio_into(
        &release_id,
        new_path.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    // Release is Local, and each file's original path names its new copy.
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Local, false)
    );
    for (file_id, name, _) in &files {
        let path = mgr.file_local_path(file_id).await.unwrap().unwrap();
        assert_eq!(
            path,
            new_path.join(name),
            "device-local path names the new copy"
        );
    }
}

/// A failed audio detachment (a missing cloud file 404s on the cache-miss read) leaves
/// bae's state consistent: the manager surfaces the error and the summary stays
/// Remote. Coven reports the missing file; bae rolls back the local copies.
#[tokio::test]
async fn detaching_audio_with_a_missing_file_fails_leaving_summary_remote() {
    let tmp = TempDir::new().unwrap();
    let (db, mgr, cloud) = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        &mgr,
        &source_dir,
        &[("x.flac", b"present"), ("y.flac", b"missing")],
    )
    .await;

    // Remove the SECOND file's file from the cloud so its cache-miss read 404s.
    // `files` carries `(file_id, original_filename, plaintext)`.
    support::remove_uploaded_file(&mgr, &cloud, &files[1].0).await;

    let new_path = tmp.path().join("exported");
    let result = mgr
        .detach_release_audio_into(
            &release_id,
            new_path.to_str().unwrap(),
            &CancellationToken::new(),
        )
        .await;
    assert!(result.is_err(), "missing file must be a hard error");

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
    subscription: &StorageBrowseSubscription,
    release_id: &str,
) -> bae_core::album_detail::ReleaseSummary {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), subscription.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves");
        if let Some(row) = value
            .windows
            .into_iter()
            .flat_map(|window| window.rows)
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
    let (_db, mgr, cloud) = setup_manager(&tmp).await;
    // The status subscription folds coven's connection lifecycle into the
    // sync values the storage view re-resolves on.
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = create_local_release(&mgr, &source_dir, &[("a.flac", b"a")]).await;

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
            selected: Default::default(),
        },
    );
    let before = next_storage_release(&values, &release_id).await;
    assert!(
        before.storage_actions.is_empty(),
        "no cloud home → no storage actions"
    );

    support::connect_storage(&mgr, &cloud).await;
    // The disk's answer for the release's originals is a value of its own,
    // and may land before or after the cloud home's.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !next_storage_release(&values, &release_id)
            .await
            .storage_actions
            .contains(&ReleaseStorageAction::Upload)
        {}
    })
    .await
    .expect("cloud home present → upload available");
}
