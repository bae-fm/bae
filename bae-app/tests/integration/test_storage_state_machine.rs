//! Release rows always sync. These tests exercise the manager's atomic file
//! attachment, durable upload queue, verified local copies, and pin state.
//! A release becomes Remote when attached, including while uploads are pending.
//! Pins independently keep its audio available offline; covers remain shared.

use bae_app::live_reads::LiveStorageBrowse;
use bae_library::LibraryManager;
use bae_store::SortDirection;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use bae_views::album_detail::ReleaseStorageState;
use bae_views::StorageBrowseRequest;
use bae_views::{StorageFilter, StorageSortCriterion, StorageSortField};
use coven::MemoryStorage;
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_manager(
    tmp: &TempDir,
) -> (bae_library::test_libraries::TestLibrary, Arc<MemoryStorage>) {
    bae_library::test_libraries::storage_library(
        tmp.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await
}

async fn setup_with_cloud(
    tmp: &TempDir,
) -> (bae_library::test_libraries::TestLibrary, Arc<MemoryStorage>) {
    let (library, storage) = setup_manager(tmp).await;
    bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
        .await;
    (library, storage)
}

/// A Remote (cloud-only, not pinned) release whose files sit in `cloud`: create it
/// Local, attach its files, and finish the uploads. Returns (release_id,
/// [(file_id, original_filename, plaintext)]).
async fn create_remote_cloud_only_release(
    db: &Store,
    mgr: &LibraryManager,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    create_remote_cloud_only_release_in_folder(
        db,
        mgr,
        bae_library::test_libraries::TEST_RELEASE_FOLDER,
        source_dir,
        files,
    )
    .await
}

/// [`create_remote_cloud_only_release`] for a release imported from the
/// folder `folder_name`.
async fn create_remote_cloud_only_release_in_folder(
    db: &Store,
    mgr: &LibraryManager,
    folder_name: &str,
    source_dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> (String, Vec<(String, String, Vec<u8>)>) {
    let (release_id, _named) =
        crate::create_local_release_in_folder(mgr, folder_name, source_dir, files).await;

    let mut captured = Vec::new();
    for file in mgr.get_files_for_release(&release_id).await.unwrap() {
        let plaintext = files
            .iter()
            .find(|(n, _)| *n == file.original_filename)
            .map(|(_, d)| d.to_vec())
            .unwrap();
        captured.push((file.id.clone(), file.original_filename.clone(), plaintext));
    }

    mgr.upload_release(&release_id).await.unwrap();
    mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(db.queued_upload_count_for_test().await.unwrap(), 0);
    assert_eq!(
        crate::release_storage(db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false },
        "release is in storage once the transition is accepted"
    );
    (release_id, captured)
}

/// Queue `release_ids` to be moved to this device into `folder` and wait until the
/// download queue has run them: `Ok` when it holds none waiting or running
/// and none failed, else the first failure's error.
async fn move_to_this_device_and_wait(
    mgr: &bae_library::test_libraries::TestLibrary,
    release_ids: &[String],
    folder: &std::path::Path,
) -> Result<(), String> {
    mgr.manager().move_releases_to_this_device(release_ids.to_vec(), folder).await.unwrap();
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let rows = mgr.manager().download_snapshot().await.unwrap().rows;
            if rows
                .iter()
                .all(|row| matches!(row.op.state, bae_transfers::DownloadPhase::Failed { .. }))
            {
                return rows.into_iter().find_map(|row| match row.op.state {
                    bae_transfers::DownloadPhase::Failed { error } => Some(Err(error)),
                    _ => None,
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the download queue runs the moves-to-this-device")
    .unwrap_or(Ok(()))
}

// ---------------------------------------------------------------------------
// Multiple releases: completing one doesn't flip another.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_releases_independent_completion() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;

    let (release_a, _a) = crate::create_local_release(
        mgr.manager(),
        &tmp.path().join("a"),
        &[("track.flac", b"aaa")],
    )
    .await;
    let (release_b, _b) = crate::create_local_release(
        mgr.manager(),
        &tmp.path().join("b"),
        &[("track.flac", b"bbb")],
    )
    .await;
    mgr.manager().upload_release(&release_a).await.unwrap();
    mgr.manager().upload_release(&release_b).await.unwrap();

    assert_eq!(mgr.manager().count_pending_uploads_for_release(&release_a).await.unwrap(), 1);
    assert_eq!(mgr.manager().count_pending_uploads_for_release(&release_b).await.unwrap(), 1);
    assert_eq!(mgr.manager().drain_uploads_expecting_work().await.unwrap(), 2);
    assert_eq!(mgr.manager().count_pending_uploads_for_release(&release_a).await.unwrap(), 0);
    assert_eq!(mgr.manager().count_pending_uploads_for_release(&release_b).await.unwrap(), 0);

    for release_id in [&release_a, &release_b] {
        assert_eq!(
            crate::release_storage(mgr.store(), release_id).await,
            ReleaseStorageState::InStorage { pinned: false }
        );
    }
}

// ---------------------------------------------------------------------------
// upload requires configured cloud storage.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_manage_refused_without_cloud_storage() {
    let tmp = TempDir::new().unwrap();
    // No storage connected, so the sync loop is NOT running.
    let (mgr, _cloud) = setup_manager(&tmp).await;
    let db = mgr.store().clone();
    assert!(
        !mgr.manager().is_sync_ready().await.unwrap(),
        "precondition: no storage / sync loop not running"
    );
    assert!(!mgr.store().syncs().await.unwrap(), "precondition: the library does not sync");
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = crate::create_local_release(
        mgr.manager(),
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    // No configured destination: refuse before attaching any files.
    let result = mgr
        .manager()
        .upload_releases(std::slice::from_ref(&release_id))
        .await
        .expect("the batch reports a per-release refusal");
    let bae_library::UploadReleasesOutcome::Partial { receipt: None, failure } = result else {
        panic!("upload must refuse the release without a receipt");
    };
    assert_eq!(failure.release_ids, vec![release_id.clone()]);
    assert!(
        db.queued_upload_count_for_test().await.unwrap() == 0,
        "no upload may be enqueued when the pipeline can't drain it"
    );
    assert_eq!(crate::release_storage(&db, &release_id).await, ReleaseStorageState::OnOneDevice);
}

// ---------------------------------------------------------------------------
// A release stays Local while its uploads are queued: rows name a file only
// once it is stored, so the release turns Remote when the last upload lands.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn attached_release_turns_remote_once_its_uploads_finish() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = crate::create_local_release(
        mgr.manager(),
        &source_dir,
        &[("a.flac", b"cloud-only-a"), ("b.flac", b"cloud-only-b")],
    )
    .await;

    mgr.manager().upload_release(&release_id).await.unwrap();

    // Queued uploads leave every row without a stored file.
    assert_eq!(crate::release_storage(&db, &release_id).await, ReleaseStorageState::OnOneDevice);

    let count = mgr.manager().drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, files.len());

    // Each completed upload names its file, and the pin state is unchanged.
    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false }
    );
}

/// Upload failure leaves the release Local and its files queued for retry.
#[tokio::test]
async fn upload_failure_keeps_attachment_and_pending_work() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) = crate::create_local_release(
        mgr.manager(),
        &source_dir,
        &[("a.flac", b"survive-a"), ("b.flac", b"survive-b")],
    )
    .await;

    mgr.manager().upload_release(&release_id).await.unwrap();

    // The connect bootstrap already wrote through the home; arm the failure only
    // now, so the upload drain fails every file.
    cloud.set_online(false);
    let count = mgr.manager().drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, 0, "no upload should succeed");

    assert_eq!(crate::release_storage(&db, &release_id).await, ReleaseStorageState::OnOneDevice);
    assert_ne!(mgr.manager().count_pending_uploads_for_release(&release_id).await.unwrap(), 0);
}

/// An original changed since import fails its release's move: the move
/// queues without reading any original, and its upload finds the change
/// before sending a byte. Nothing is stored, the move's record names the
/// changed original, and the source files remain untouched.
#[tokio::test]
async fn truncated_source_fails_its_move_without_sending() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = crate::create_local_release(
        mgr.manager(),
        &source_dir,
        &[("a.flac", b"full-length-bytes"), ("b.flac", b"second-file")],
    )
    .await;

    // Corrupt one original: fewer bytes on disk than its registered size.
    let changed = source_dir.join("a.flac");
    tokio::fs::write(&changed, b"x").await.unwrap();

    mgr.manager().upload_release(&release_id).await.unwrap();
    assert_eq!(mgr.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    assert_eq!(db.queued_upload_count_for_test().await.unwrap(), 0);
    let pending = mgr.manager().sync_status().await.unwrap().pending;
    assert!(
        pending.iter().any(|item| matches!(
            &item.wait.reason,
            coven::PendingReason::Original {
                failure: coven::FileSourceFailure::Changed,
                path,
            } if *path == changed
        )),
        "{pending:?}"
    );
    for (name, _) in files {
        assert!(source_dir.join(name).exists());
    }
    assert_eq!(crate::release_storage(&db, &release_id).await, ReleaseStorageState::OnOneDevice);
}

// ---------------------------------------------------------------------------
// audio detachment from Remote: bae's summary reads Local and each file's device-local
// path names its new copy. The cloud objects the release stops naming are
// coven's to retire through accepted reclaim.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detaching_audio_updates_summary_and_original_paths() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        mgr.manager(),
        &source_dir,
        &[("x.flac", b"download-x"), ("y.flac", b"download-yy")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    std::fs::create_dir(&new_path).unwrap();
    move_to_this_device_and_wait(&mgr, std::slice::from_ref(&release_id), &new_path).await.unwrap();

    // Release is Local, and each file's original path names its new copy in
    // the release's own folder.
    assert_eq!(crate::release_storage(&db, &release_id).await, ReleaseStorageState::OnOneDevice);
    for (file_id, name, _) in &files {
        let path = mgr.manager().file_local_path(file_id).await.unwrap().unwrap();
        assert_eq!(
            path,
            new_path.join(bae_library::test_libraries::TEST_RELEASE_FOLDER).join(name),
            "device-local path names the new copy"
        );
    }
}

/// Two releases moved to this device into one picked folder each go in a folder of
/// their own, named for the release, so their `01.flac` do not collide.
#[tokio::test]
async fn moving_two_releases_to_this_device_puts_each_in_its_own_folder() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let mut releases = Vec::new();
    for (name, bytes) in [("Name A", b"first".as_slice()), ("Name B", b"second".as_slice())] {
        let (release_id, _files) = create_remote_cloud_only_release_in_folder(
            &db,
            mgr.manager(),
            name,
            &tmp.path().join(format!("originals-{name}")),
            &[("01.flac", bytes)],
        )
        .await;
        releases.push((release_id, name));
    }

    let folder = tmp.path().join("exported");
    std::fs::create_dir(&folder).unwrap();
    let ids: Vec<String> = releases.iter().map(|(release_id, _)| release_id.clone()).collect();
    move_to_this_device_and_wait(&mgr, &ids, &folder).await.unwrap();

    for ((release_id, name), bytes) in releases.iter().zip([b"first".as_slice(), b"second"]) {
        assert_eq!(
            std::fs::read(folder.join(name).join("01.flac")).unwrap(),
            bytes,
            "{name} has its own folder"
        );
        assert_eq!(crate::release_storage(&db, release_id).await, ReleaseStorageState::OnOneDevice);
    }
}

/// A failed audio detachment (a missing cloud file 404s on its download) leaves
/// bae's state consistent: the manager surfaces the error and the summary stays
/// Remote. Coven reports the missing file and removes the copies it made.
#[tokio::test]
async fn detaching_audio_with_a_missing_file_fails_leaving_summary_remote() {
    let tmp = TempDir::new().unwrap();
    let (mgr, cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let source_dir = tmp.path().join("originals");
    let (release_id, files) = create_remote_cloud_only_release(
        &db,
        mgr.manager(),
        &source_dir,
        &[("x.flac", b"present"), ("y.flac", b"missing")],
    )
    .await;

    // Remove the SECOND file's file from the cloud so its cache-miss read 404s.
    // `files` carries `(file_id, original_filename, plaintext)`.
    bae_library::test_libraries::remove_uploaded_file(mgr.manager(), &cloud, &files[1].0).await;

    let new_path = tmp.path().join("exported");
    std::fs::create_dir(&new_path).unwrap();
    let result =
        move_to_this_device_and_wait(&mgr, std::slice::from_ref(&release_id), &new_path).await;
    assert!(result.is_err(), "missing file must fail the entry");

    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false }
    );
    assert_eq!(std::fs::read_dir(&new_path).unwrap().count(), 0, "no copy is left");
}

/// Two names of a release that coven writes as one name (`?` becomes its
/// full-width lookalike) can't both be placed in the folder: making the
/// release local is refused before anything downloads, and it stays in the
/// cloud with no copy left behind.
#[tokio::test]
async fn detaching_audio_whose_names_write_as_one_is_refused() {
    let tmp = TempDir::new().unwrap();
    let (mgr, _cloud) = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let (release_id, _files) = create_remote_cloud_only_release(
        &db,
        mgr.manager(),
        &tmp.path().join("originals"),
        &[("Why?.flac", b"first"), ("Why\u{FF1F}.flac", b"second")],
    )
    .await;

    let new_path = tmp.path().join("exported");
    std::fs::create_dir(&new_path).unwrap();
    let error = move_to_this_device_and_wait(&mgr, std::slice::from_ref(&release_id), &new_path)
        .await
        .expect_err("two copies at one path can't both be made");

    assert!(error.contains("name one path"), "{error}");
    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false }
    );
    assert_eq!(std::fs::read_dir(&new_path).unwrap().count(), 0, "no copy is made");
}

// ---------------------------------------------------------------------------
// Storage reactivity: the storage value joins the database row with the
// current sync state, so connecting a home delivers the changed actions.
// ---------------------------------------------------------------------------

async fn next_storage_release(
    subscription: &LiveStorageBrowse,
    release_id: &str,
) -> bae_views::album_detail::ReleaseSummary {
    loop {
        let value = tokio::time::timeout(HANG_GUARD, subscription.next())
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
async fn storage_subscription_delivers_actions_when_storage_is_set_up() {
    use bae_types::release_storage::ReleaseStorageAction;

    let tmp = TempDir::new().unwrap();
    let (mgr, cloud) = setup_manager(&tmp).await;
    // The status subscription folds coven's connection lifecycle into the
    // sync values the storage view re-resolves on.
    let source_dir = tmp.path().join("originals");
    let (release_id, _files) =
        crate::create_local_release(mgr.manager(), &source_dir, &[("a.flac", b"a")]).await;

    let views = bae_views::LibraryViews::new(mgr.store().clone());
    let values = bae_app::live_reads::subscribe_storage_browse(
        mgr.manager(),
        &views,
        StorageBrowseRequest {
            sort: StorageSortCriterion {
                field: StorageSortField::AlbumTitle,
                direction: SortDirection::Ascending,
            },
            filter: StorageFilter::All,
            search: String::new(),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
            selected: Default::default(),
        },
    );
    let before = next_storage_release(&values, &release_id).await;
    assert!(before.storage_actions.is_empty(), "no storage → no storage actions");

    bae_library::test_libraries::connect_storage(mgr.manager(), mgr.store(), &cloud).await;
    // The disk's answer for the release's originals is a value of its own,
    // and may land before or after storage's.
    tokio::time::timeout(HANG_GUARD, async {
        while !next_storage_release(&values, &release_id)
            .await
            .storage_actions
            .contains(&ReleaseStorageAction::Upload)
        {}
    })
    .await
    .expect("storage set up → upload available");
}
