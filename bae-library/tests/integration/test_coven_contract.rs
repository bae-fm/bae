//! The actual bae schema must satisfy coven's declaration and file contracts.

#[tokio::test]
async fn real_schema_opens_with_the_declarations_and_device_only_files() {
    let directory = tempfile::tempdir().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(directory.path()).unwrap().coven_app_dir());
    let sources = bae_library::test_libraries::TestSources::system();
    let handle = bae_store::configure_store(sources.builder(&installation), bae_library::SCHEMA)
        .create_store("Schema Test")
        .await
        .unwrap();
    let records = bae_records::Records::new(
        bae_store::test_devices::store_under(handle.clone(), sources.clock().clone()),
        sources.ids().clone(),
    );
    let artist = bae_records::DbArtist {
        id: coven::IdSource::new_id(&**sources.ids()).to_string(),
        name: "Schema Artist".into(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    let album = bae_records::DbAlbum {
        id: coven::IdSource::new_id(&**sources.ids()).to_string(),
        title: "Schema Album".into(),
        year: None,
        is_compilation: false,
        created_at: chrono::Utc::now(),
    };
    let release = bae_records::DbRelease::new_test(
        &album.id,
        &coven::IdSource::new_id(&**sources.ids()).to_string(),
    );
    let credit = bae_records::DbAlbumArtist::new_test(&album.id, &artist.id);
    records
        .insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();
    let file = bae_records::NewReleaseFile::new(
        &release.id,
        "track.flac",
        bae_util::content_type::ContentType::Flac,
        bae_records::ReleaseFileRole::Other,
        coven::IdSource::new_id(&**sources.ids()).to_string(),
        chrono::Utc::now(),
    );
    let original = directory.path().join("track.flac");
    std::fs::write(&original, vec![0; 42]).unwrap();
    records.insert_file_for_test(&file, &original).await.unwrap();
    let stored = records.find_file_by_id(&file.id).await.unwrap().unwrap();
    assert!(!stored.stored);
    assert_eq!(stored.file_size, 42);
    assert!(handle.uploads().await.unwrap().files.is_empty());
    handle.close().await.unwrap();
}

#[test]
fn creation_retains_its_keyring_failure_through_cleanup() {
    let operation = coven::StoreCreationError::Identity(coven::IdentityError::Custody(
        coven::KeyError::Unsupported,
    ));
    let error = bae_library::CreateLibraryError::Store(coven::StoreCreationError::Cleanup {
        operation: Box::new(operation),
        cleanup: Box::new(coven::StoreCreationError::Sync(coven::SyncError::NoStorage)),
    });
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::Keyring);
}

#[tokio::test]
async fn upload_cancellation_keeps_the_file_on_this_device_with_its_original() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = bae_library::test_libraries::storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    // A move to the cloud needs storage set up on this device; uploads stay
    // paused, so the move is still queued when it is cancelled.
    bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
        .await;
    let records = library.records().clone();
    let artist = bae_records::DbArtist {
        id: coven::IdSource::new_id(&**library.ids()).to_string(),
        name: "Artist".into(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    let album = bae_records::DbAlbum {
        id: coven::IdSource::new_id(&**library.ids()).to_string(),
        title: "Album".into(),
        year: None,
        is_compilation: false,
        created_at: chrono::Utc::now(),
    };
    let release = bae_records::DbRelease::new_test(
        &album.id,
        &coven::IdSource::new_id(&**library.ids()).to_string(),
    );
    let credit = bae_records::DbAlbumArtist::new_test(&album.id, &artist.id);
    records
        .insert_album_with_release_and_tracks(&album, &[credit], &release, &[], &[], &[])
        .await
        .unwrap();
    let source = directory.path().join("track.flac");
    std::fs::write(&source, b"audio").unwrap();
    let file = bae_records::NewReleaseFile::new(
        &release.id,
        "track.flac",
        bae_util::content_type::ContentType::Flac,
        bae_records::ReleaseFileRole::Other,
        coven::IdSource::new_id(&**library.ids()).to_string(),
        chrono::Utc::now(),
    );
    library.manager().add_external_file_for_test(&file, &source).await.unwrap();
    library.manager().upload_release(&release.id, false).await.unwrap();
    assert_eq!(library.store().uploads().await.unwrap().files.len(), 1, "the file waits to upload");
    library.manager().cancel_release_transition(&release.id).await.unwrap();
    // Cancelling the move retires its uploads; the file stays on this
    // device only, read from its original.
    assert!(library.store().uploads().await.unwrap().files.is_empty());
    assert!(library
        .manager()
        .get_files_for_release(&release.id)
        .await
        .unwrap()
        .iter()
        .all(|file| !file.stored));
    assert_eq!(library.manager().file_local_path(&file.id).await.unwrap(), Some(source));
    library.store().close().await.unwrap();
}
