use super::*;

/// An import goes where the stored storage choice says when it starts; the
/// caller names only the candidate.
#[tokio::test]
async fn an_import_goes_where_the_stored_choice_says_as_it_starts() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let collection = f.temp_path().join("Collection");
    let mut keys = Vec::new();
    for title in ["Album One", "Album Two"] {
        let album = collection.join(format!("Artist - {title}"));
        fs::create_dir_all(&album).unwrap();
        generate_tagged_album_files(
            &album,
            title,
            "Artist",
            None,
            &[TaggedTrack { filename: "01 Track.flac", title: "Track", track_number: 1 }],
        );
        keys.push(bae_import::CandidateKey::folder(&album));
    }
    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle.add_watched_folder(collection.to_path_buf()).await.unwrap();
    // The scan reads both folders at once, so either can be found first.
    let mut unfound: std::collections::HashSet<&bae_import::CandidateKey> = keys.iter().collect();
    wait_for_scan_event(&mut scan_rx, "both albums", |event| {
        if let ScanEvent::FolderCandidate { candidate: c, .. } = event {
            unfound.remove(&bae_import::CandidateKey::folder(&c.path));
        }
        unfound.is_empty()
    })
    .await;
    for key in &keys {
        f.handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }

    f.library.set_import_to_cloud(true).await.unwrap();
    f.library.set_import_pinned(true).await.unwrap();
    let outcome = import_outcome(&f, &keys[0]).await;
    let bae_import::ImportProgress::RemoteUploadQueued { id, .. } = outcome else {
        panic!("the cloud choice queues the release's upload: {outcome:?}");
    };
    // The import stages its files to be kept here: no entry of the download
    // queue pins them, and once the move finishes coven keeps each whole.
    assert!(
        !f.library
            .download_snapshot()
            .await
            .unwrap()
            .rows
            .iter()
            .any(|row| row.op.release_id == id),
        "a kept import needs no download"
    );
    // No sync loop runs behind this connection, so the test drives the
    // release's upload itself.
    assert_eq!(
        f.library.drain_uploads_expecting_work().await.unwrap(),
        1,
        "the release's one file uploads"
    );
    f.library.wait_for_kept_release_for_test(&id).await;
    let storage = bae_views::LibraryViews::new(f.library.store().clone())
        .find_release_detail(&id)
        .await
        .unwrap()
        .unwrap()
        .summary;
    assert_eq!(
        storage.storage_state,
        bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true },
        "the import keeps the selected offline pin"
    );

    f.library.set_import_to_cloud(false).await.unwrap();
    assert!(
        matches!(import_outcome(&f, &keys[1]).await, bae_import::ImportProgress::Complete { .. }),
        "the local choice completes the release where it is"
    );
}

/// Start `key`'s import and return its first `Complete` or `RemoteUploadQueued`.
async fn import_outcome(
    f: &ImportFixture,
    key: &bae_import::CandidateKey,
) -> bae_import::ImportProgress {
    let import_id = f.handle.start_import(key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    match bae_import::test_imports::wait_for_import_end(&mut progress_rx).await {
        bae_import::ImportProgress::Failed { error, .. } => {
            panic!("the import of {key} failed: {error}")
        }
        outcome => outcome,
    }
}

/// A local import's cover is bae's own image, which coven queues for upload
/// even with no storage connected. It is not the release uploading: with
/// several local releases imported and nothing connected, no release has an
/// upload, none sorts or filters as uploading, and the covers wait among the
/// library's other files.
#[tokio::test]
async fn a_local_import_s_queued_cover_is_not_its_release_uploading() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let mut release_ids = Vec::new();
    for title in ["Album One", "Album Two", "Album Three"] {
        let album = collection.join(format!("Artist - {title}"));
        fs::create_dir_all(&album).unwrap();
        generate_tagged_album_files_with_embedded_cover(
            &album,
            title,
            "Artist",
            &[TaggedTrack { filename: "01 Track.flac", title: "Track", track_number: 1 }],
        );
        let (id, _) = import_folder(
            &f,
            &album,
            None,
            ImportDestination::Local,
            bae_import::test_imports::DraftSource::FileTags,
        )
        .await
        .expect("a local import commits with no storage connected");
        assert!(
            f.library.database().cover_version(&id).await.unwrap().is_some(),
            "the import stores the embedded cover"
        );
        release_ids.push(id);
    }

    assert_eq!(
        f.library.database().queued_upload_count_in_table_for_test("release_files").await.unwrap(),
        0,
        "no release's files wait to upload"
    );
    assert_eq!(
        f.library.database().queued_upload_count_in_table_for_test("covers").await.unwrap(),
        release_ids.len() as i64,
        "each cover waits to upload"
    );
    assert!(f.library.database().uploading_releases().await.unwrap().is_empty());
}
