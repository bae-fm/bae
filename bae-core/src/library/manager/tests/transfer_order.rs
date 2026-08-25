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
        display_title: "Album Title".to_string(),
        files: Vec::new(),
        progress: crate::library::UploadProgress::default(),
    };
    let snapshot = crate::library::OutboxSnapshot {
        upload_groups: vec![group("z-first"), group("a-second")],
        ..Default::default()
    };

    assert_eq!(
        snapshot.transitioning_release_ids(),
        vec!["z-first", "a-second"],
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_remote_enqueues_cover_then_files_in_display_order() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let mut album = create_test_album();
    album.title = "Album Title".to_string();
    manager.database.insert_album(&album).await.unwrap();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_release(&release).await.unwrap();
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
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            id.to_string(),
            created_at,
            crate::util::fs::hash_bytes(bytes),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.to_string_lossy())
        .await
        .unwrap();
    let cover_bytes = b"cover-bytes";
    let cover = DbLibraryImage::cover(
        &release.id,
        &Uuid::new_v4().to_string(),
        "local",
        None,
        cover_bytes,
        manager.clock.now(),
    );
    manager
        .store_library_image_blob(&cover, cover_bytes)
        .await
        .unwrap();
    let files = manager
        .database
        .get_files_for_release(&release.id)
        .await
        .unwrap();

    manager.coven_make_remote(&release.id, false).await.unwrap();

    assert_eq!(
        manager
            .database
            .queued_upload_rows_for_root_for_test("releases", &release.id)
            .await
            .unwrap(),
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
        ],
    );
}
