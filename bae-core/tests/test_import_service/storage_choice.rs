/// An import goes where the stored storage choice says when it starts; the
/// caller names only the candidate.
#[tokio::test]
async fn an_import_goes_where_the_stored_choice_says_as_it_starts() {
    support::tracing_init();
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
    // The scan reads both folders at once, so either can be found first.
    let mut unfound: std::collections::HashSet<&str> = keys.iter().map(String::as_str).collect();
    wait_for_scan_event(&mut scan_rx, "both albums", |event| {
        if let ScanEvent::FolderCandidate { candidate: c, .. } = event {
            unfound.remove(c.path.to_string_lossy().as_ref());
        }
        unfound.is_empty()
    })
    .await;
    for key in &keys {
        f.handle
            .select_candidate_file_tags(key.clone())
            .await
            .unwrap();
    }

    f.library_manager.set_import_to_cloud(true).await.unwrap();
    f.library_manager.set_import_pinned(true).await.unwrap();
    f.library_manager.set_downloads_paused(true).await.unwrap();
    let outcome = import_outcome(&f, &keys[0]).await;
    let bae_core::import::ImportProgress::RemoteUploadQueued { id, .. } = outcome else {
        panic!("the cloud choice queues the release's upload: {outcome:?}");
    };
    assert!(
        f.library_manager
            .download_snapshot()
            .await
            .unwrap()
            .rows
            .iter()
            .any(|row| row.op.release_id == id),
        "the committed import queues its selected offline copy"
    );
    f.library_manager.set_downloads_paused(false).await.unwrap();
    let mut downloads = f.library_manager.subscribe_download_snapshot();
    let settled = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let queue = downloads
                .next()
                .await
                .expect("the query stays open")
                .unwrap();
            if queue.rows.iter().all(|row| {
                row.op.release_id != id
                    || matches!(row.op.state, bae_core::library::DownloadPhase::Failed { .. })
            }) {
                return queue;
            }
        }
    })
    .await
    .expect("the selected offline copy finishes");
    assert!(
        !settled.rows.iter().any(|row| row.op.release_id == id),
        "offline copy failed: {settled:?}"
    );
    let storage = f
        .library_manager
        .find_release_storage_summary(&id)
        .await
        .unwrap()
        .unwrap();
    assert!(storage.pinned, "the import keeps the selected offline pin");

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
    let import_id = f.handle.start_import(key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    match support::wait_for_import_end(&mut progress_rx).await {
        bae_core::import::ImportProgress::Failed { error, .. } => {
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
    support::tracing_init();
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
            &[TaggedTrack {
                filename: "01 Track.flac",
                title: "Track",
                track_number: 1,
            }],
        );
        let (id, _) = import_folder(
            &f,
            &album,
            None,
            ImportDestination::Local,
            support::DraftSource::FileTags,
        )
        .await
        .expect("a local import commits with no storage connected");
        assert!(
            f.db.cover_version(&id).await.unwrap().is_some(),
            "the import stores the embedded cover"
        );
        release_ids.push(id);
    }

    let outbox = f.library_manager.outbox_state().await.unwrap();
    assert!(
        outbox.queue.upload_groups.is_empty(),
        "no release is uploading: {:?}",
        outbox.queue.upload_groups
    );
    assert!(outbox.queue.per_release_progress().is_empty());
    assert_eq!(
        outbox
            .queue.other_files
            .iter()
            .filter(|file| file.label == bae_core::library::UploadFileLabel::Cover)
            .count(),
        release_ids.len(),
        "each cover waits among the library's other files"
    );
    assert!(f.db.uploading_releases().await.unwrap().is_empty());
}
