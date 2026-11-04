//! Moving releases to the cloud: admission, the upload queue, what a failed or
//! cancelled move leaves, retrying, and the storage list's view of an upload.

use super::*;
use crate::manager::tests::*;
use bae_db::{DbLibraryImage, DbRelease, NewReleaseFile};
use chrono::Utc;
use tempfile::TempDir;

/// Cancelling every move needs no original: nothing was switched, so every
/// release stays on this device only, and one whose original went shows it
/// missing.
#[tokio::test]
async fn cancelling_uploads_with_a_missing_original_keeps_every_release_here() {
    let (manager, temp, first, _) = queued_upload_fixture("first").await;
    let second = insert_release_with_queued_uploads(
        &manager,
        &temp.path().join("second"),
        "Album",
        &[("a.flac", b"first"), ("b.flac", b"second")],
    )
    .await;
    std::fs::remove_file(temp.path().join("second/b.flac")).unwrap();
    manager.cancel_all_release_uploads().await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    for release in [&first, &second] {
        for file in release_files(&manager, &release.id).await {
            assert!(!file.stored);
            assert!(manager.database.external_file(&file.id).await.unwrap().is_some());
        }
    }
    assert_eq!(
        views(&manager).find_release_detail(&second.id).await.unwrap().unwrap().playability,
        bae_views::album_detail::ReleasePlayability::Playable,
        "the release is back on this device, as the library records it"
    );
}

#[tokio::test]
async fn cancelling_every_upload_beside_a_new_cover_preserves_completed_audio_and_its_download() {
    let (manager, _temp, release, _) = queued_upload_fixture("completed").await;
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release.id.clone()]).await.unwrap();
    store_test_cover(&manager, &release.id, b"cover").await;
    assert_eq!(download_queue_now(&manager).await.rows.len(), 1);
    manager.cancel_all_release_uploads().await.unwrap();
    assert_eq!(download_queue_now(&manager).await.rows.len(), 1);
    assert!(release_files(&manager, &release.id).await.iter().all(|file| file.stored));
    cover_file(&manager, &release.id).await;
}

/// Attaching files requires a configured provider; absence is a setup issue.
#[tokio::test]
async fn making_a_release_remote_without_storage_requires_configuration() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let transfer = crate::storage::transfer::ReleaseTransfer::new(
        manager.library.clone(),
        release.id,
        ReleaseStorageAction::Upload,
    );
    let error = transfer.start().await.expect_err("storage must be configured");
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::Config, "{error}");
}

/// Uploading reads this device's originals, so every surface that offers it —
/// the release detail, its storage summary, and the Storage Manager row —
/// offers it only for a release the library records this device holding the
/// originals of: not for a release another device imported. The disk is not
/// asked: an original gone since import still offers it, and the move then
/// fails reading the file.
#[test]
fn upload_is_offered_only_where_this_device_holds_the_originals() {
    crate::test_libraries::run_two_device_test(async |devices| {
        let first = TempDir::new().unwrap();
        let elsewhere = release_on_the_first_device(devices, first.path(), "Album B").await;
        let originals = TempDir::new().unwrap();
        check_upload_offered(devices.b().manager(), originals.path(), &elsewhere).await;
    });
}

async fn check_upload_offered(
    manager: &LibraryManager,
    originals: &std::path::Path,
    elsewhere: &DbRelease,
) {
    use bae_types::release_storage::ReleaseStorageAction;
    let here = insert_local_release_with_files(
        manager,
        originals,
        "Album A",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    let offered = |release_id: String| async move {
        let detail = bae_views::LibraryViews::new(manager.database.clone())
            .find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("the release exists")
            .summary
            .storage_actions;
        let row = storage_page(manager, bae_views::StorageFilter::All, 0, 10)
            .await
            .rows
            .into_iter()
            .find(|row| row.release.id == release_id)
            .expect("the release is listed")
            .release
            .storage_actions;
        assert_eq!(detail, row, "the detail and the storage row agree");
        detail
    };

    assert_eq!(offered(here.id.clone()).await, vec![ReleaseStorageAction::Upload]);
    assert_eq!(offered(elsewhere.id.clone()).await, Vec::new());
    std::fs::remove_file(originals.join("track.flac")).unwrap();
    assert_eq!(offered(here.id.clone()).await, vec![ReleaseStorageAction::Upload]);
}

/// The Storage Manager row of a release with files waiting to upload says
/// how many wait, offers only cancelling, and reads as settled once they
/// have uploaded.
#[tokio::test]
async fn a_storage_row_reads_its_queued_upload_from_the_queue() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("queued"),
        "Album Title",
        &[("a.flac", b"bytes-a"), ("b.flac", b"bytes-b")],
    )
    .await;
    let row = |manager: LibraryManager, release_id: String| async move {
        bae_views::LibraryViews::new(manager.database.clone())
            .get_storage_page(
                &bae_views::StorageSortCriterion {
                    field: bae_views::StorageSortField::AlbumTitle,
                    direction: bae_db::SortDirection::Ascending,
                },
                bae_views::StorageFilter::All,
                0,
                50,
            )
            .await
            .unwrap()
            .rows
            .into_iter()
            .find(|row| row.release.id == release_id)
            .expect("the release is listed")
    };
    let summary = |row: bae_views::album_detail::StorageRow| row.release;
    let queued = summary(row(manager.library.clone(), release.id.clone()).await);
    assert_eq!(
        queued.upload,
        Some(bae_views::album_detail::ReleaseUpload {
            progress: bae_db::UploadProgress { queued: 2, uploading: 0 },
            summary: vec![bae_transfers::TransferCount {
                label: bae_types::count_label::CountLabel {
                    key: "core.queue.queued".into(),
                    count: 2
                },
                stage: bae_transfers::TransferStage::Waiting,
            }],
        })
    );
    assert!(queued.transitioning);
    assert!(queued.storage_actions.is_empty());

    manager.drain_uploads_expecting_work().await.unwrap();
    let uploaded = summary(row(manager.library.clone(), release.id.clone()).await);
    assert_eq!(uploaded.upload, None);
    assert!(!uploaded.transitioning);
}

/// A file being sent counts as uploading in its release's row while its
/// upload is in flight.
#[tokio::test]
async fn a_storage_row_counts_the_file_being_sent() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut browse = views(&manager).subscribe_storage_browse(bae_views::StorageBrowseRequest {
        sort: bae_views::StorageSortCriterion {
            field: bae_views::StorageSortField::AlbumTitle,
            direction: bae_db::SortDirection::Ascending,
        },
        filter: bae_views::StorageFilter::All,
        search: String::new(),
        windows: [bae_db::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        selected: Default::default(),
    });
    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = browse.next().await.expect("the browse query answers");
            let projection = event.result.unwrap();
            let sending = projection.windows.iter().flat_map(|window| &window.rows).any(|row| {
                row.release.id == release.id
                    && row.release.upload.is_some_and(|upload| upload.sending == 1)
            });
            if sending {
                break;
            }
        }
    })
    .await
    .expect("the row counts the file being sent");
    drain.await.unwrap().unwrap();
}

/// The sync activity counts the file being sent as its send starts, while
/// nothing joins or leaves the queue, and drops it as the send ends.
#[tokio::test]
async fn the_sync_activity_follows_a_send_as_it_starts_and_ends() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("sending"),
        "Album Title",
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let mut activity = views(&manager.library).subscribe_sync_activity();
    assert_eq!(crate::followed::value_matching(&mut activity, |_| true).await.uploads, None);
    // Held in flight long enough that its start is read while it runs.
    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_for_test().await }
    });
    let sending = loop {
        let next = crate::followed::value_matching(&mut activity, |_| true).await;
        if let Some(uploads) = next.uploads {
            break uploads;
        }
    };
    assert_eq!(sending, bae_views::UploadsInFlight { sending: 1, waiting: 0 });
    while crate::followed::value_matching(&mut activity, |_| true).await.uploads.is_some() {}
    drain.await.unwrap().unwrap();
}

/// The releases uploading run in the order their uploads were admitted, not
/// in their titles' order.
#[tokio::test]
async fn uploading_releases_keep_their_admission_order() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    let first = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("first-upload"),
        "Album Z",
        &[("track.flac", b"first")],
    )
    .await;
    let second = insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("second-upload"),
        "Album A",
        &[("track.flac", b"second")],
    )
    .await;

    assert_eq!(manager.database.uploading_releases().await.unwrap(), vec![first.id, second.id],);
}

#[tokio::test]
async fn upload_keeps_every_file_in_the_queue() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let mut album = create_test_album();
    album.title = "Album Title".to_string();
    manager.database.insert_album(&album).await.unwrap();
    let release = create_test_release(&album.id);
    insert_release(&manager, &release).await;
    let source_dir = temp_dir.path().join("ordered-upload");
    std::fs::create_dir_all(&source_dir).unwrap();
    let created_at = Utc::now();
    for (id, name, bytes) in [
        ("00000000-0000-4000-8000-000000000001", "Track 10.flac", b"ten".as_slice()),
        ("ffffffff-ffff-4fff-bfff-ffffffffffff", "track 2.flac", b"two".as_slice()),
    ] {
        let path = source_dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = NewReleaseFile::new(
            &release.id,
            name,
            bae_util::content_type::ContentType::Flac,
            id.to_string(),
            created_at,
        );
        manager.add_external_file_for_test(&file, &path).await.unwrap();
    }
    let files = release_files(&manager, &release.id).await;

    manager.upload_release(&release.id, false).await.unwrap();

    assert_eq!(
        manager
            .database
            .queued_upload_rows_for_root_for_test("releases", &release.id)
            .await
            .unwrap()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        vec![
            (bae_db::synced::RELEASE_FILES_NAMESPACE.to_string(), files[0].id.clone(),),
            (bae_db::synced::RELEASE_FILES_NAMESPACE.to_string(), files[1].id.clone(),),
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>(),
    );
}

/// Playing a local release another device holds says which device its audio
/// is on: nobody started an upload, so there is nothing here to wait for.
#[test]
fn playing_a_file_on_another_device_names_the_device() {
    crate::test_libraries::run_two_device_test(async |devices| {
        use bae_playback::PlaybackErrorReason;
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        let manager = devices.b().manager();

        let files = release_files(manager, &release.id).await;
        let file = files.first().expect("the release has a file");
        let reason = playback_error_reason_for_file(manager, file).await;
        assert_eq!(
            reason,
            PlaybackErrorReason::OnOtherDevice(bae_db::OtherDevice::Active {
                name: "Device A".to_string()
            })
        );
    });
}

/// The read layer surfaces a local release another device holds — there is
/// no availability filter to hide one. Every release is shared, including
/// one whose files have not been uploaded.
#[test]
fn surfaces_a_local_release_another_device_holds() {
    crate::test_libraries::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        check_release_surfaces(devices.b().manager(), &release).await;
    });
}

async fn check_release_surfaces(manager: &LibraryManager, release: &DbRelease) {
    let album_id = release.album_id.clone();

    // Grid and count include the album.
    let views = bae_views::LibraryViews::new(manager.database.clone());
    let page = views.get_album_page(&[], 0, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].release_ids, vec![release.id.clone()]);
    assert_eq!(views.get_album_count().await.unwrap(), 1);

    // Album detail carries the release.
    let detail = views.find_album_detail(&album_id).await.unwrap().expect("album surfaces");
    let detail_ids: Vec<_> = detail.releases.iter().map(|r| r.summary.id.clone()).collect();
    assert_eq!(detail_ids, vec![release.id.clone()]);

    // The release-level resolver returns it.
    assert!(bae_views::LibraryViews::new(manager.database.clone())
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn cancelling_a_completed_upload_preserves_the_attachment() {
    let (manager, temp, release, _) = queued_upload_fixture("completed").await;
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.cancel_release_transition(&release.id).await.unwrap();
    assert!(release_files(&manager, &release.id).await.iter().all(|file| file.stored));
    assert_eq!(std::fs::read(temp.path().join("completed/a.flac")).unwrap().len(), 1000);
}

/// A missing original makes its move wait, and cancelling the move needs no
/// original: the file stays on this device only, with nothing pending.
#[tokio::test]
async fn a_missing_upload_source_is_pending_until_its_move_is_cancelled() {
    let (manager, temp, release, file_id) = queued_upload_fixture("queued").await;
    assert_eq!(manager.count_pending_uploads_for_release(&release.id).await.unwrap(), 1);
    std::fs::remove_file(temp.path().join("queued/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    assert!(!manager.pending().await.unwrap().is_empty());
    manager.cancel_release_upload(&release.id).await.unwrap();
    assert!(!manager.database.find_file_by_id(&file_id).await.unwrap().unwrap().stored);
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.pending().await.unwrap().is_empty());
}

#[tokio::test]
async fn cancelling_all_release_uploads_restores_their_originals() {
    let (manager, temp, first, _) = queued_upload_fixture("first").await;
    let second = insert_release_with_queued_uploads(
        &manager,
        &temp.path().join("second"),
        "Album",
        &[("b.flac", b"audio")],
    )
    .await;
    manager.cancel_all_release_uploads().await.unwrap();
    // Cancelling retires each move, so its uploads leave the queue.
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.pending().await.unwrap().is_empty());
    for release in [first, second] {
        for file in release_files(&manager, &release.id).await {
            assert!(!file.stored);
            assert!(
                manager.database.external_file(&file.id).await.unwrap().is_some(),
                "the original plays again"
            );
            assert_eq!(
                manager.database.read_release_file(&file.id).await.unwrap().len(),
                std::fs::metadata(
                    manager.database.external_file(&file.id).await.unwrap().unwrap().path
                )
                .unwrap()
                .len() as usize
            );
        }
    }
}

/// Deleting the row of a file still uploading deletes the file with its last
/// reference: its upload leaves the queue and nothing is left pending.
#[tokio::test]
async fn an_upload_whose_row_is_deleted_leaves_the_queue() {
    let (manager, _temp, _release, file_id) = queued_upload_fixture("changed").await;
    manager
        .database
        .execute_for_test("DELETE FROM release_files WHERE id = ?1", vec![file_id])
        .await
        .unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    assert!(manager.pending().await.unwrap().is_empty());
}

/// An original that changes before its upload fails its release's move: the
/// pending list names the file and offers Cancel upload, which ends the move,
/// and the release stays on this device only.
#[tokio::test]
async fn a_changed_original_fails_the_move_until_its_upload_is_cancelled() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("changed").await;
    std::fs::write(temp.path().join("changed/a.flac"), b"changed after the move").unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status();
    let listed = pending_list_until(&mut values, |items| {
        items
            .iter()
            .any(|item| matches!(item.wait.action, bae_db::PendingAction::CancelUpload { .. }))
    })
    .await;
    let item = listed
        .iter()
        .find(|item| matches!(item.wait.action, bae_db::PendingAction::CancelUpload { .. }))
        .unwrap();
    assert_eq!(
        item.subject,
        bae_db::PendingSubjectName::ReleaseFile {
            file_name: "a.flac".into(),
            album_title: "Test Album".into(),
        }
    );
    let bae_db::PendingAction::CancelUpload { id } = item.wait.action.clone() else {
        unreachable!("found by its action");
    };
    manager.cancel_failed_release_move(id).await.unwrap();
    for file in release_files(&manager, &release.id).await {
        assert!(!file.stored, "the release stays on this device");
    }
    assert_eq!(manager.count_pending_uploads_for_release(&release.id).await.unwrap(), 0);
    pending_list_until(&mut values, |items| {
        !items
            .iter()
            .any(|item| matches!(item.wait.action, bae_db::PendingAction::CancelUpload { .. }))
    })
    .await;
}

/// An upload whose original vanished is listed in the sync status, named by
/// its file and album, with restoring the file as what the person can do.
#[tokio::test]
async fn an_upload_missing_its_original_is_listed_by_name() {
    let (manager, temp, _release, _file_id) = queued_upload_fixture("listed").await;
    std::fs::remove_file(temp.path().join("listed/a.flac")).unwrap();
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 0);
    manager.start().unwrap();
    let mut values = manager.subscribe_sync_status();
    let items = pending_list_until(&mut values, |items| !items.is_empty()).await;
    let item = items
        .iter()
        .find(|item| matches!(item.subject, bae_db::PendingSubjectName::ReleaseFile { .. }))
        .expect("the upload is named by its file");
    assert_eq!(
        item.subject,
        bae_db::PendingSubjectName::ReleaseFile {
            file_name: "a.flac".into(),
            album_title: "Test Album".into(),
        }
    );
    assert_eq!(item.wait.action, bae_db::PendingAction::RestoreFile);
}

/// A release's cover, carried by the `covers` row whose key is the release
/// id, is no file of the release's upload: its reference is bound, so it
/// uploads through the pause that keeps the release's file waiting.
#[tokio::test]
async fn a_releases_cover_is_no_file_of_its_upload() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("cover-upload"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let cover_bytes = b"cover-bytes";
    let cover = DbLibraryImage::cover(&release.id);
    manager.store_library_image(&cover, cover_bytes).await.unwrap();
    manager.upload_release(&release.id, false).await.unwrap();
    finish_bound_uploads(&manager).await;

    assert_eq!(
        manager.count_pending_uploads_for_release(&release.id).await.unwrap(),
        1,
        "the release's upload is its own file"
    );
    assert_eq!(
        manager.database.queued_upload_count_in_table_for_test("covers").await.unwrap(),
        0,
        "the cover uploads through the pause"
    );
    assert!(matches!(
        cover_file(&manager, &release.id).await.place(),
        coven::FilePlace::Cloud { .. }
    ));
}

/// An upload command has queued every release it admitted before it
/// returns, so the release's row reads its upload as the command's caller
/// finishes.
#[tokio::test]
async fn an_upload_queues_each_release_before_returning() {
    let (manager, temp_dir) = setup_test_manager().await;
    // Uploads paused: nothing drains the queue behind the test.
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("publish-queue"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let second_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("publish-queue-second"),
        "Second Album Title",
        &[("second.flac", b"second-track-bytes")],
    )
    .await;

    let outcome =
        manager.upload_releases(&[release.id.clone(), second_release.id.clone()]).await.unwrap();
    let UploadReleasesOutcome::Complete { receipt } = outcome else {
        panic!("both releases must be admitted");
    };
    assert_eq!(receipt.release_ids, vec![release.id.clone(), second_release.id.clone()]);
    for release in [release, second_release] {
        assert!(has_pending_uploads(&manager, &release.id).await);
    }
}

#[tokio::test]
async fn an_active_target_does_not_block_sibling_cloud_admissions() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let first_new_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("first-new-target"),
        "First Album",
        &[("first.flac", b"first-bytes")],
    )
    .await;
    let active_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("active-target"),
        "Second Album",
        &[("second.flac", b"second-bytes")],
    )
    .await;
    let last_new_release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("last-new-target"),
        "Third Album",
        &[("third.flac", b"third-bytes")],
    )
    .await;
    assert!(
        bae_transfers::release_storage_action::begin_release_storage_action(
            &manager.database,
            &active_release.id,
            bae_transfers::StoredTransition::Unpin
        )
        .await
        .unwrap(),
        "hold the target in another transition"
    );

    let outcome = manager
        .upload_releases(&[
            first_new_release.id.clone(),
            active_release.id.clone(),
            last_new_release.id.clone(),
        ])
        .await
        .expect("the batch returns every release's admission outcome");
    let UploadReleasesOutcome::Partial { receipt: Some(receipt), failure } = outcome else {
        panic!("two releases must be admitted around one refusal");
    };
    assert_eq!(
        receipt.release_ids,
        vec![first_new_release.id.clone(), last_new_release.id.clone()]
    );
    assert_eq!(failure.release_ids, vec![active_release.id.clone()]);
    let bae_diagnostics::UiError::Diagnostic { category, detail } = failure.error;
    assert_eq!(category, bae_diagnostics::UiErrorCategory::Internal);
    assert!(detail.contains(&active_release.id));

    for release in [first_new_release, last_new_release] {
        assert_eq!(moving_file_count(&manager, &release.id).await, 1);
    }
}

#[tokio::test]
async fn an_upload_batch_refuses_duplicate_release_ids_without_admission() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("duplicate-target"),
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    manager
        .upload_releases(&[release.id.clone(), release.id.clone()])
        .await
        .expect_err("a duplicate target refuses the whole batch");

    assert_eq!(moving_file_count(&manager, &release.id).await, 0);
}

/// A user retry without a connected cloud must report the refusal to its
/// caller; returning success would leave the failed queue unchanged while the
/// Storage Manager claims the retry ran.
#[tokio::test]
async fn retry_outbox_now_surfaces_a_missing_cloud_connection() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let error = manager
        .retry_outbox_now()
        .await
        .expect_err("retrying without a cloud connection must fail");

    assert!(matches!(error, LibraryError::Sync(_)), "got {error:?}");
}

#[tokio::test]
async fn upload_pause_survives_retry_and_resuming_allows_progress() {
    let (manager, _temp, _release, _) = queued_upload_fixture("paused").await;
    manager.set_sync_paused(true).await.unwrap();
    // Sync runs, so passes make requests of their own; the retry sends no
    // file.
    let before = manager.storage.request_trace().len();
    assert!(matches!(
        manager.database.retry_uploads_now().await.unwrap(),
        coven::DrainOutcome::Paused
    ));
    let files = coven::ObjectPrefix::files(manager.database.store_id_for_test());
    let sent: Vec<_> = manager.storage.request_trace()[before..]
        .iter()
        .filter(
            |request| matches!(request, coven::MemoryRequest::Create(path) if files.contains(path)),
        )
        .cloned()
        .collect();
    assert!(sent.is_empty(), "a paused retry sent {sent:?}");
    assert!(manager.is_sync_paused().await.unwrap());
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
}

#[tokio::test]
async fn retry_outbox_surfaces_a_provider_failure_and_can_be_retried() {
    let (manager, _temp, _release, _) = queued_upload_fixture("retry").await;
    manager.storage.set_online(false);
    manager.set_sync_paused(false).await.unwrap();
    manager.retry_outbox_now().await.unwrap();
    assert!(!manager.pending().await.unwrap().is_empty());
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 1);
    manager.storage.set_online(true);
    manager.retry_outbox_now().await.unwrap();
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
}

/// A move to the cloud keeps the release here as the stored import storage
/// choice says at the moment it moves, with nothing passed beside the releases.
#[tokio::test]
async fn upload_pins_as_the_stored_choice_says() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let kept = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("kept"),
        "Album Title",
        &[("track.flac", b"kept-bytes")],
    )
    .await;
    let evictable = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("evictable"),
        "Other Album Title",
        &[("track.flac", b"evictable-bytes")],
    )
    .await;

    manager.set_import_pinned(true).await.unwrap();
    manager.upload_releases(std::slice::from_ref(&kept.id)).await.unwrap();
    manager.set_import_pinned(false).await.unwrap();
    manager.upload_releases(std::slice::from_ref(&evictable.id)).await.unwrap();

    manager.drain_uploads_expecting_work().await.unwrap();
    wait_for_kept_release(&manager, &kept.id).await;
    let pinned = |release_id: String| {
        let manager = manager.library.clone();
        async move {
            bae_views::LibraryViews::new(manager.database.clone())
                .find_release_detail(&release_id)
                .await
                .unwrap()
                .expect("the library holds the release")
                .summary
                .storage_state
                == bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
        }
    };
    assert!(pinned(kept.id.clone()).await);
    assert!(!pinned(evictable.id.clone()).await);
}
