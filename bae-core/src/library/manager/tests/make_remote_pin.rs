/// A move to the cloud keeps the release here as the stored import storage
/// choice says at the moment it moves, with nothing passed beside the releases.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_remote_pins_as_the_stored_choice_says() {
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
        .make_releases_remote(std::slice::from_ref(&kept.id))
        .await
        .unwrap();
    manager.set_import_pinned(false).await.unwrap();
    manager
        .make_releases_remote(std::slice::from_ref(&evictable.id))
        .await
        .unwrap();

    let queue = manager.database.outbox_queue().await.unwrap();
    let retained = |release_id: &str| {
        queue
            .make_remotes
            .iter()
            .find(|entry| entry.transition.root_id == release_id)
            .map(|entry| entry.transition.retain_pinned)
    };
    assert_eq!(retained(&kept.id), Some(true));
    assert_eq!(retained(&evictable.id), Some(false));
}
