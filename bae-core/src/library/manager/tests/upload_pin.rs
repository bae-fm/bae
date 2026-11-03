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
            crate::library::views::LibraryViews::new(manager.database.clone())
                .find_release_detail(&release_id)
                .await
                .unwrap()
                .expect("the library holds the release")
                .summary
                .pinned
        }
    };
    assert!(pinned(kept.id.clone()).await);
    assert!(!pinned(evictable.id.clone()).await);
}

/// A release kept here as it moves to the cloud is pinned only once its
/// files are whole on this device. The write switching its files to the cloud
/// asks coven to keep each, and coven downloads its copy from storage; until
/// that copy lands, the Storage Manager's list and the release's own summary
/// both read the release as not pinned, and the copy landing pins it.
#[tokio::test]
async fn a_kept_release_is_pinned_once_its_copy_lands() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = release_kept_here_waiting_for_its_copy(
        &manager,
        &temp_dir.path().join("kept"),
        "Album Title",
    )
    .await;
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    let file = manager.release_file_ref(&file_id).await.unwrap();
    assert_eq!(
        manager.database.files_pinned(&[file.id()]).await.unwrap(),
        [Some(false)],
        "the file's copy hasn't landed"
    );
    let summary_pinned = || async {
        manager
            .views()
            .find_release_detail(&release.id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .summary
            .pinned
    };
    assert!(!summary_pinned().await, "the release's summary");
    assert!(
        !listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );

    home.set_faults(coven::Faults::none()).await;
    manager.database.pin(std::slice::from_ref(&file)).await.unwrap();
    assert!(summary_pinned().await, "the release's summary");
    assert!(
        listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );
}

/// A release whose kept copies haven't landed sorts with the releases in the
/// cloud only, and as pinned once they land.
#[tokio::test]
async fn a_kept_release_sorts_as_pinned_once_its_copies_land() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let cloud =
        uploaded_release(&manager, &temp_dir.path().join("cloud"), "Album Title A", false).await;
    let waiting = release_kept_here_waiting_for_its_copy(
        &manager,
        &temp_dir.path().join("waiting"),
        "Album Title B",
    )
    .await;
    let by_storage = || async {
        crate::library::views::LibraryViews::new(manager.database.clone())
            .get_db_storage_page(
                &crate::library::views::StorageSortCriterion {
                    field: crate::library::views::StorageSortField::Storage,
                    direction: bae_db::SortDirection::Ascending,
                },
                crate::library::views::StorageFilter::All,
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
        [(cloud.clone(), StoragePlace::Cloud), (waiting.id.clone(), StoragePlace::Cloud),]
    );

    home.set_faults(coven::Faults::none()).await;
    let refs = release_file_refs(&manager, &waiting.id).await;
    manager.database.pin(&refs).await.unwrap();
    assert_eq!(
        by_storage().await,
        [(waiting.id.clone(), StoragePlace::Pinned), (cloud.clone(), StoragePlace::Cloud),]
    );
    inspector_rows_until(&manager, &waiting.id, |rows| {
        rows.files.iter().all(|file| file.pin == Some(crate::album_detail::FilePin::Kept))
    })
    .await;
}
