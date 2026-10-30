/// The releases uploading run in the order their uploads were admitted, not
/// in their titles' order.
#[cfg(feature = "test-utils")]
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

    assert_eq!(
        manager.database.uploading_releases().await.unwrap(),
        vec![first.id, second.id],
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn upload_keeps_the_cover_and_every_file_in_the_queue() {
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
        (
            "00000000-0000-4000-8000-000000000001",
            "Track 10.flac",
            b"ten".as_slice(),
        ),
        (
            "ffffffff-ffff-4fff-bfff-ffffffffffff",
            "track 2.flac",
            b"two".as_slice(),
        ),
    ] {
        let path = source_dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            bae_util::content_type::ContentType::Flac,
            id.to_string(),
            created_at,
        );
        manager
            .add_external_file_for_test(&file, &path)
            .await
            .unwrap();
    }
    let cover_bytes = b"cover-bytes";
    let cover = DbLibraryImage::cover(&release.id, "local", None, manager.clock.now().into());
    manager
        .store_library_image(&cover, cover_bytes)
        .await
        .unwrap();
    let files = release_files(&manager, &release.id).await;

    manager.upload_release(&release.id, false).await.unwrap();

    assert_eq!(
        manager
            .database
            .queued_upload_rows_for_root_for_test("releases", &release.id)
            .await
            .unwrap().into_iter().collect::<std::collections::BTreeSet<_>>(),
        vec![
            (crate::sync::COVERS_NAMESPACE.to_string(), release.id),
            (
                crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
                files[0].id.clone(),
            ),
            (
                crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
                files[1].id.clone(),
            ),
        ].into_iter().collect::<std::collections::BTreeSet<_>>(),
    );
}
