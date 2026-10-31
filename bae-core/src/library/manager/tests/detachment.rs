use super::*;

#[test]
fn cancelling_queued_audio_keeps_only_this_device_playable() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        check_release_detachment(devices, false).await
    });
}

#[test]
fn making_uploaded_audio_local_keeps_only_this_device_playable() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        check_release_detachment(devices, true).await
    });
}

async fn check_release_detachment(devices: &crate::sync::test_devices::TwoDevices, uploaded: bool) {
    devices.pause().await;
    let manager = devices.a().manager();
    manager.database.set_uploads_paused(true).await.unwrap();
    let original = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_in_album(
        manager,
        &album.id,
        original.path(),
        &[("a.flac", b"first audio"), ("b.flac", b"second audio")],
    )
    .await;
    store_test_cover(manager, &release.id, b"cover").await;
    let cover = cover_file(manager, &release.id).await.id();
    manager.upload_release(&release.id, false).await.unwrap();
    let files = release_files(manager, &release.id).await;
    assert_eq!(files.len(), 2);
    assert_eq!(attached_file_count(manager, &release.id).await, 2);
    assert_eq!(
        manager
            .count_pending_uploads_for_release(&release.id)
            .await
            .unwrap(),
        2
    );

    if uploaded {
        manager.database.set_uploads_paused(false).await.unwrap();
        devices.resume().await;
        devices
            .converge(
                "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        assert_eq!(
            manager
                .count_pending_uploads_for_release(&release.id)
                .await
                .unwrap(),
            0
        );
        // The second device has cached the audio; detachment must still prevent playback.
        for file in &files {
            let expected = std::fs::read(original.path().join(&file.original_filename)).unwrap();
            assert_eq!(
                devices
                    .b()
                    .database()
                    .read_release_file(&file.id)
                    .await
                    .unwrap(),
                expected
            );
        }
        devices.pause().await;
        manager.database.unlock_store_key().await.unwrap();
        manager
            .detach_release_audio(&release.id, destination.path())
            .await
            .unwrap();
    } else {
        manager
            .cancel_release_transition(&release.id)
            .await
            .unwrap();
        // Retrying the command cannot undo the local state.
        manager
            .cancel_release_transition(&release.id)
            .await
            .unwrap();
    }

    assert_eq!(attached_file_count(manager, &release.id).await, 0);
    for file in &files {
        assert!(
            !manager
                .database
                .find_file_by_id(&file.id)
                .await
                .unwrap()
                .unwrap()
                .stored
        );
        let path = if uploaded {
            destination.path()
        } else {
            original.path()
        }
        .join(&file.original_filename);
        assert_eq!(manager.file_local_path(&file.id).await.unwrap(), Some(path));
        let expected = std::fs::read(original.path().join(&file.original_filename)).unwrap();
        assert_eq!(
            manager.database.read_release_file(&file.id).await.unwrap(),
            expected
        );
    }
    assert_eq!(cover_file(manager, &release.id).await.id(), cover);
    manager.database.set_uploads_paused(false).await.unwrap();
    devices.resume().await;
    devices
        .converge(
            "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
        )
        .await;
    for file in &files {
        assert!(
            !devices
                .b()
                .database()
                .find_file_by_id(&file.id)
                .await
                .unwrap()
                .unwrap()
                .stored
        );
        assert!(devices
            .b()
            .manager()
            .file_local_path(&file.id)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            devices.b().database().read_release_file(&file.id).await,
            Err(crate::db::DbError::FileNotUploaded(_))
        ));
        assert!(matches!(
            playback_error_reason_for_file(devices.b().manager(), file).await,
            crate::ui::PlaybackErrorReason::NotUploaded
        ));
    }
}

/// Cancelling the move of a release to be kept here makes it local again:
/// nothing is left to keep, and it plays from its original.
#[tokio::test]
async fn cancelling_the_upload_of_a_release_kept_here_detaches_audio() {
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        temp.path(),
        "Album",
        &[("track.flac", b"audio")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    manager
        .cancel_release_transition(&release.id)
        .await
        .unwrap();
    assert!(download_queue_now(&manager).await.rows.is_empty());
    assert_eq!(
        manager
            .database
            .queued_upload_count_for_test()
            .await
            .unwrap(),
        0
    );
    let files = release_files(&manager, &release.id).await;
    assert_eq!(attached_file_count(&manager, &release.id).await, 0);
    assert_eq!(
        manager
            .database
            .read_release_file(&files[0].id)
            .await
            .unwrap(),
        b"audio"
    );
}

#[tokio::test]
async fn cancelling_uploads_with_a_missing_original_changes_no_release() {
    let (manager, temp, first, _) = queued_upload_fixture("first").await;
    let second = insert_release_with_queued_uploads(
        &manager,
        &temp.path().join("second"),
        "Album",
        &[("a.flac", b"first"), ("b.flac", b"second")],
    )
    .await;
    std::fs::remove_file(temp.path().join("second/b.flac")).unwrap();
    manager
        .cancel_all_release_uploads()
        .await
        .expect_err("every original is required before detaching");
    for release in [first, second] {
        let files = release_files(&manager, &release.id).await;
        assert_eq!(
            attached_file_count(&manager, &release.id).await,
            files.len()
        );
        for file in files {
            assert!(!file.stored);
            assert!(manager
                .database
                .external_file(&file.id)
                .await
                .unwrap()
                .is_some());
        }
    }
}

#[tokio::test]
async fn cancelling_a_cover_upload_preserves_completed_audio_and_its_download() {
    let (manager, _temp, release, _) = queued_upload_fixture("completed").await;
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.set_downloads_paused(true).await.unwrap();
    manager
        .enqueue_pins(vec![release.id.clone()])
        .await
        .unwrap();
    store_test_cover(&manager, &release.id, b"cover").await;
    assert_eq!(download_queue_now(&manager).await.rows.len(), 1);
    manager.cancel_all_release_uploads().await.unwrap();
    assert_eq!(download_queue_now(&manager).await.rows.len(), 1);
    assert!(release_files(&manager, &release.id)
        .await
        .iter()
        .all(|file| file.stored));
    cover_file(&manager, &release.id).await;
}
