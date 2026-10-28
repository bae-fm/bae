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
    let release =
        release_waiting_for_its_carried_pin(&manager, &temp_dir.path().join("carried"), "Album Title")
            .await;
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

/// A release whose files' pins wait for coven to fetch their copies is
/// being pinned: the Storage Manager shows it pinning and sorts it with the
/// releases pinning, ahead of those only in the cloud, and its inspector
/// shows each file waiting and coven's fetch counting them, until its copies
/// land and it is pinned.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_release_waiting_for_its_carried_pins_is_pinning() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let cloud = uploaded_release(&manager, &temp_dir.path().join("cloud"), "Album Title A", false).await;
    let waiting =
        release_waiting_for_its_carried_pin(&manager, &temp_dir.path().join("waiting"), "Album Title B")
            .await;
    let by_storage = || async {
        manager
            .database
            .get_storage_page(
                &crate::db::StorageSortCriterion {
                    field: crate::db::StorageSortField::Storage,
                    direction: crate::db::SortDirection::Ascending,
                },
                crate::db::StorageFilter::All,
                0,
                100,
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.release.id, row.place))
            .collect::<Vec<_>>()
    };
    use crate::album_detail::StoragePlace;
    assert_eq!(
        by_storage().await,
        [
            (waiting.id.clone(), StoragePlace::Pinning),
            (cloud.clone(), StoragePlace::Cloud),
        ]
    );
    use crate::album_detail::FilePin;
    use crate::library::storage_inspector::{PinFiles, StorageInspectorTransfer};
    let services = crate::library::AppServices::for_test(manager.library.clone())
        .await
        .unwrap();
    let inspector = services.subscribe_storage_inspector();
    inspector.set(Some(waiting.id.clone())).unwrap();
    let fetching = inspector_until(&inspector, |_| true).await;
    assert_eq!(
        fetching.transfers,
        [StorageInspectorTransfer::PinFetch {
            files: PinFiles { kept: 0, waiting: 1 }
        }]
    );
    assert!(fetching.files.iter().all(|file| file.pin == Some(FilePin::Waiting)));

    home.set_faults(coven::Faults::none()).await;
    let refs = release_file_refs(&manager, &waiting.id).await;
    manager.database.pin(&refs).await.unwrap();
    assert_eq!(
        by_storage().await,
        [
            (waiting.id.clone(), StoragePlace::Pinned),
            (cloud.clone(), StoragePlace::Cloud),
        ]
    );
    let pinned = inspector_until(&inspector, |value| value.transfers.is_empty()).await;
    assert!(pinned
        .files
        .iter()
        .all(|file| file.pin.is_none() && file.file.pin == Some(FilePin::Kept)));
}
