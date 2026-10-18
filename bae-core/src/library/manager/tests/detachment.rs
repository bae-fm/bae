use super::*;

#[test]
fn cancelling_queued_audio_keeps_only_this_device_playable() {
    crate::sync::test_devices::run_two_device_test(|| check_release_detachment(false));
}

#[test]
fn making_uploaded_audio_local_keeps_only_this_device_playable() {
    crate::sync::test_devices::run_two_device_test(|| check_release_detachment(true));
}

async fn check_release_detachment(uploaded: bool) {
    let devices = crate::sync::test_devices::TwoDevices::new().await;
    devices.pause().await;
    let manager = devices.a().manager();
    manager.database.set_uploads_paused(true).await.unwrap();
    let original = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let album = create_test_album();
    manager
        .database
        .insert_artist(&DbArtist {
            id: album.artist_id.clone(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: album.created_at,
        })
        .await
        .unwrap();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_in_album(
        manager,
        &album.id,
        original.path(),
        &[("a.flac", b"first audio"), ("b.flac", b"second audio")],
    )
    .await;
    store_test_cover(manager, &release.id, b"cover").await;
    let cover = manager
        .database
        .file_ref("covers", &release.id)
        .await
        .unwrap();
    manager.coven_make_remote(&release.id, false).await.unwrap();
    let files = release_files(manager, &release.id).await;
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file.attached));
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
            .converge("SELECT id, hex(hash), location FROM release_files")
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
            .make_release_local(&release.id, destination.path().to_str().unwrap())
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

    for file in &files {
        assert!(
            !manager
                .database
                .find_file_by_id(&file.id)
                .await
                .unwrap()
                .unwrap()
                .attached
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
    assert_eq!(
        manager
            .database
            .file_ref("covers", &release.id)
            .await
            .unwrap(),
        cover
    );
    manager.database.set_uploads_paused(false).await.unwrap();
    devices.resume().await;
    devices
        .converge("SELECT id, hex(hash), location FROM release_files")
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
                .attached
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
            crate::ui::PlaybackErrorReason::UploadPending
        ));
    }
    devices.a().manager().close().await.unwrap();
    devices.b().manager().close().await.unwrap();
}

#[tokio::test]
async fn cancelling_upload_with_a_queued_offline_copy_detaches_audio() {
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    manager.set_downloads_paused(true);
    let release = insert_local_release_with_files(
        &manager,
        temp.path(),
        "Album",
        &[("track.flac", b"audio")],
    )
    .await;
    manager.coven_make_remote(&release.id, true).await.unwrap();
    assert_eq!(manager.download_queue().ops.len(), 1);
    manager
        .cancel_release_transition(&release.id)
        .await
        .unwrap();
    assert!(manager.download_queue().ops.is_empty());
    let files = release_files(&manager, &release.id).await;
    assert!(files.iter().all(|file| !file.attached));
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
        for file in release_files(&manager, &release.id).await {
            assert!(file.attached);
            let query = format!(
                "SELECT path FROM local_release_files WHERE file_id='{}'",
                file.id
            );
            assert!(manager
                .database
                .query_texts_for_test(&query)
                .await
                .unwrap()
                .is_empty());
        }
    }
}

#[tokio::test]
async fn cancelling_a_cover_upload_preserves_completed_audio_and_its_download() {
    let (manager, _temp, release, _) = queued_upload_fixture("completed").await;
    manager.drain_uploads_expecting_work().await.unwrap();
    manager.set_downloads_paused(true);
    manager.enqueue_pins(vec![release.id.clone()]).await;
    store_test_cover(&manager, &release.id, b"cover").await;
    assert_eq!(manager.download_queue().ops.len(), 1);
    manager.cancel_all_release_uploads().await.unwrap();
    assert_eq!(manager.download_queue().ops.len(), 1);
    assert!(release_files(&manager, &release.id)
        .await
        .iter()
        .all(|file| file.attached));
    assert!(manager
        .database
        .file_ref("covers", &release.id)
        .await
        .is_ok());
}
