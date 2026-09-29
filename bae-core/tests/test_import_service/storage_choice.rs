/// An import goes where the stored storage choice says when it starts; the
/// caller names only the candidate.
#[tokio::test]
async fn an_import_goes_where_the_stored_choice_says_as_it_starts() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    f.library_manager
        .set_test_cloud_provider(bae_core::config::CloudProvider::Dropbox)
        .await;

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
            &[TaggedTrack {
                filename: "01 Track.flac",
                title: "Track",
                track_number: 1,
            }],
        );
        keys.push(album.to_string_lossy().into_owned());
    }
    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    for key in &keys {
        wait_for_scan_event(&mut scan_rx, key, |event| {
            matches!(event, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(key.as_str()))
        })
        .await;
        f.handle
            .select_candidate_file_tags(key.clone())
            .await
            .unwrap();
    }

    f.library_manager.set_import_to_cloud(true).await.unwrap();
    assert!(
        matches!(
            import_outcome(&f, &keys[0]).await,
            bae_core::import::ImportProgress::RemoteUploadQueued { .. }
        ),
        "the cloud choice queues the release's upload"
    );

    f.library_manager.set_import_to_cloud(false).await.unwrap();
    assert!(
        matches!(
            import_outcome(&f, &keys[1]).await,
            bae_core::import::ImportProgress::Complete { .. }
        ),
        "the local choice completes the release where it is"
    );
}

/// Start `key`'s import and return its first `Complete` or `RemoteUploadQueued`.
async fn import_outcome(f: &ImportFixture, key: &str) -> bae_core::import::ImportProgress {
    let import_id = f.handle.start_import(key).await.unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    match support::wait_for_import_end(&mut progress_rx).await {
        bae_core::import::ImportProgress::Failed { error, .. } => {
            panic!("the import of {key} failed: {error}")
        }
        outcome => outcome,
    }
}
