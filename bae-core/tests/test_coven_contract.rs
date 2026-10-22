#![cfg(feature = "test-utils")]
//! The actual bae schema must satisfy coven's declaration and file contracts.

#[tokio::test]
async fn real_schema_opens_with_the_declarations_and_unattached_files() {
    let directory = tempfile::tempdir().unwrap();
    let installation = coven::TestCoven::new();
    let handle = installation
        .builder(
            bae_core::config::AppDir::at(directory.path())
                .unwrap()
                .store_layout(),
        )
        .synced_tables(bae_core::sync::synced_tables())
        .migrations(bae_core::migrations::all())
        .create_store("Schema Test")
        .await
        .unwrap();
    let database = bae_core::db::Database::from_handle(
        handle.clone(),
        std::sync::Arc::new(coven::SystemClock),
    );
    let artist = bae_core::db::DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Schema Artist".into(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();
    let album = bae_core::db::DbAlbum {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Schema Album".into(),
        year: None,
        is_compilation: false,
        created_at: chrono::Utc::now(),
    };
    let release = bae_core::db::DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string());
    let credit = bae_core::db::DbAlbumArtist::new_test(&album.id, &artist.id);
    database
        .insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();
    let file = bae_core::db::DbFile::new(
        &release.id,
        "track.flac",
        42,
        bae_core::util::content_type::ContentType::Flac,
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
    );
    database.insert_file(&file).await.unwrap();
    let stored = database.find_file_by_id(&file.id).await.unwrap().unwrap();
    assert!(!stored.stored);
    assert!(handle.uploads().await.unwrap().files.is_empty());
    handle.close().await.unwrap();
}

#[test]
fn creation_retains_its_keyring_failure_through_cleanup() {
    let operation = coven::StoreCreationError::Identity(coven::IdentityError::Custody(
        coven::KeyError::UnsupportedKeyringPlatform,
    ));
    let error = bae_core::library::CreateLibraryError::Store(coven::StoreCreationError::Cleanup {
        operation: Box::new(operation),
        cleanup: Box::new(coven::StoreCreationError::Sync(coven::SyncError::NoStorage)),
    });
    assert_eq!(error.category(), bae_core::ui::UiErrorCategory::Keyring);
}

#[tokio::test]
async fn upload_cancellation_detaches_the_file_and_restores_its_original() {
    let directory = tempfile::tempdir().unwrap();
    let installation = coven::TestCoven::new();
    let handle = installation
        .builder(
            bae_core::config::AppDir::at(directory.path())
                .unwrap()
                .store_layout(),
        )
        .synced_tables(bae_core::sync::synced_tables())
        .migrations(bae_core::migrations::all())
        .create_store("Upload Test")
        .await
        .unwrap();
    let (manager, database) = bae_test_support::test_library_from_handle(
        handle.clone(),
        directory.path(),
        bae_core::providers::Providers::offline(),
    );
    let artist = bae_core::db::DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Artist".into(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();
    let album = bae_core::db::DbAlbum {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Album".into(),
        year: None,
        is_compilation: false,
        created_at: chrono::Utc::now(),
    };
    let release = bae_core::db::DbRelease::new_test(&album.id, &uuid::Uuid::new_v4().to_string());
    let credit = bae_core::db::DbAlbumArtist::new_test(&album.id, &artist.id);
    database
        .insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();
    let source = directory.path().join("track.flac");
    std::fs::write(&source, b"audio").unwrap();
    let file = bae_core::db::DbFile::new(
        &release.id,
        "track.flac",
        5,
        bae_core::util::content_type::ContentType::Flac,
        uuid::Uuid::new_v4().to_string(),
        chrono::Utc::now(),
    );
    manager
        .add_external_file_for_test(&file, &source)
        .await
        .unwrap();
    manager.upload_release(&release.id, false).await.unwrap();
    manager
        .cancel_release_transition(&release.id)
        .await
        .unwrap();
    // Clearing the pending attachment abandons it, so its upload leaves the
    // queue.
    assert!(handle.uploads().await.unwrap().files.is_empty());
    assert!(manager
        .get_files_for_release(&release.id)
        .await
        .unwrap()
        .iter()
        .all(|file| !file.stored));
    assert_eq!(
        manager.file_local_path(&file.id).await.unwrap(),
        Some(source)
    );
    handle.close().await.unwrap();
}
