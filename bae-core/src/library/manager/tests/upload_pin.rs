/// A move to the cloud keeps the release here as the stored import storage
/// choice says at the moment it moves, with nothing passed beside the releases.
#[cfg(feature = "test-utils")]
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
    manager
        .upload_releases(std::slice::from_ref(&kept.id))
        .await
        .unwrap();
    manager.set_import_pinned(false).await.unwrap();
    manager
        .upload_releases(std::slice::from_ref(&evictable.id))
        .await
        .unwrap();

    manager.drain_uploads_expecting_work().await.unwrap();
    wait_for_kept_release(&manager, &kept.id).await;
    let pinned = |release_id: String| {
        let manager = manager.library.clone();
        async move {
            manager
                .find_release_storage_summary(&release_id)
                .await
                .unwrap()
                .expect("the library holds the release")
                .pinned
        }
    };
    assert!(pinned(kept.id.clone()).await);
    assert!(!pinned(evictable.id.clone()).await);
}

/// What disconnecting would cost counts the uploaded releases this device
/// doesn't keep pinned, and no local one.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_cloud_only_count_is_the_uploaded_releases_not_pinned() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;
    upload_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("pinned"),
        "Pinned Album",
        true,
    )
    .await;
    upload_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("cloud"),
        "Cloud Album",
        false,
    )
    .await;
    insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("local"),
        "Local Album",
        &[("track.flac", b"local-bytes")],
    )
    .await;

    assert_eq!(manager.cloud_only_release_count().await.unwrap(), 1);
}

/// A release pinned as it uploads is pinned only once its files are whole on
/// this device. Completing the upload carries each file's pin to the stored
/// file, where it waits for a copy from storage; until that copy lands, the
/// Storage Manager's list, the release's own summary and what disconnecting
/// would cost all read the release as not pinned, and the copy landing pins
/// it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_carried_pin_pins_its_release_once_the_copy_lands() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("carried"),
        "Album Title",
        &[("track.flac", b"carried-bytes")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    wait_for_offline_copy_request(&manager, &release.id).await;

    // Hold the file's upload as it is created, and from then on refuse every
    // read of the stored file, so the carried pin can't fetch its copy.
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.database.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
        async move { manager.drain_uploads_expecting_work().await }
    });
    created.await.unwrap();
    let stored = home
        .request_trace()
        .into_iter()
        .rev()
        .find_map(|request| match request {
            coven::MemoryRequest::Create(path) => Some(path),
            _ => None,
        })
        .expect("the held upload is the last creation");
    home.set_faults(coven::Faults {
        read_failures: [(stored, coven::StorageFailure::Network)].into(),
        ..coven::Faults::none()
    })
    .await;
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();

    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    assert_eq!(
        manager
            .database
            .query_texts_for_test(&format!(
                "SELECT CAST(pin.kept AS TEXT) FROM coven_pins pin \
                 WHERE pin.table_name = 'release_files' AND pin.key = coven_key('{file_id}')"
            ))
            .await
            .unwrap(),
        ["0"],
        "the file's pin waits for its copy"
    );
    let summary_pinned = || async {
        manager
            .find_release_storage_summary(&release.id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .pinned
    };
    assert!(!summary_pinned().await, "the release's summary");
    assert!(
        !listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );
    assert_eq!(
        manager.cloud_only_release_count().await.unwrap(),
        1,
        "what disconnecting would cost"
    );

    home.set_faults(coven::Faults::none()).await;
    let file = manager.release_file_ref(&file_id).await.unwrap();
    manager.database.pin(std::slice::from_ref(&file)).await.unwrap();
    assert!(summary_pinned().await, "the release's summary");
    assert!(
        listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );
    assert_eq!(manager.cloud_only_release_count().await.unwrap(), 0);
}
