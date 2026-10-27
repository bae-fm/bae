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
    let kept_file = release_files(&manager, &kept.id).await[0].id.clone();
    let evictable_file = release_files(&manager, &evictable.id).await[0].id.clone();
    assert!(manager.release_pinned(Some(&kept_file)).await.unwrap());
    assert!(!manager.release_pinned(Some(&evictable_file)).await.unwrap());
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
