#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_uploading_filter_preserves_transition_admission_order() {
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

    let page = manager
        .get_storage_page(
            &crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction: crate::db::SortDirection::Ascending,
            },
            crate::db::StorageFilter::Uploading,
            0,
            10,
        )
        .await
        .unwrap();

    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.release.id.as_str())
            .collect::<Vec<_>>(),
        vec![first.id.as_str(), second.id.as_str()],
    );
}

#[test]
fn transitioning_release_ids_preserve_upload_group_order() {
    let group = |release_id: &str| crate::library::UploadReleaseGroup {
        release_id: release_id.to_string(),
        files: Vec::new(),
        progress: crate::library::UploadProgress::default(),
    };
    let snapshot = crate::library::OutboxQueue {
        upload_groups: vec![group("z-first"), group("a-second")],
        other_files: Vec::new(),
        total: crate::library::UploadProgress::default(),
        pause_state: crate::library::OutboxPauseState::Running,
    };

    assert_eq!(
        snapshot.transitioning_release_ids(),
        vec!["z-first", "a-second"],
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
    let cover = DbLibraryImage::cover(
        &release.id,
        "local",
        None,
        cover_bytes,
        manager.clock.now().into(),
        manager.new_id(),
    );
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
