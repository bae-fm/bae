/// An album folder of two tagged tracks under the fixture's temp folder.
fn two_track_album(f: &ImportFixture, title: &str) -> std::path::PathBuf {
    let album = f.temp_path().join("Collection").join(title);
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        title,
        "Artist",
        None,
        &[
            TaggedTrack {
                filename: "01 Track.flac",
                title: "Track One",
                track_number: 1,
            },
            TaggedTrack {
                filename: "02 Track.flac",
                title: "Track Two",
                track_number: 2,
            },
        ],
    );
    album
}

/// The coven file each of the release's rows references, by row.
async fn release_file_ids(f: &ImportFixture, release_id: &str) -> Vec<(String, coven::FileId)> {
    let mut files: Vec<_> = f
        .db
        .get_files_for_release(release_id)
        .await
        .unwrap()
        .into_iter()
        .map(|file| (file.id, file.file))
        .collect();
    files.sort();
    files
}

/// Every file of the release is on this device only, nothing of it waits to
/// upload, and each reads the bytes of its original under `folder`.
async fn assert_on_this_device(f: &ImportFixture, release_id: &str, folder: &Path) {
    assert_eq!(
        f.db.count_pending_uploads_for_release(release_id)
            .await
            .unwrap(),
        0,
        "nothing of the release waits to upload"
    );
    for file in f.db.get_files_for_release(release_id).await.unwrap() {
        assert!(!file.stored, "{} is on this device only", file.original_filename);
        let original = folder.join(&file.original_filename);
        assert_eq!(
            f.db.external_file(&file.id).await.unwrap().map(|original| original.path),
            Some(original.clone())
        );
        assert_eq!(
            f.library_manager
                .materialize_release_file_for_test(&file.id)
                .await
                .unwrap(),
            fs::read(&original).unwrap()
        );
    }
}

/// Move the release to the cloud through the Storage Manager's command,
/// which checks it may move first.
async fn move_to_cloud(f: &ImportFixture, release_id: &str) {
    let outcome = f
        .library_manager
        .upload_releases(&[release_id.to_string()])
        .await
        .unwrap();
    assert!(
        matches!(outcome, bae_core::library::UploadReleasesOutcome::Complete { .. }),
        "the release moves: {outcome:?}"
    );
}

/// Drain the release's move and find every file stored, each row still on
/// the file it named before.
async fn finish_move(f: &ImportFixture, release_id: &str, files: &[(String, coven::FileId)]) {
    assert_eq!(
        f.library_manager.drain_uploads_expecting_work().await.unwrap(),
        files.len(),
        "each of the release's files uploads"
    );
    assert!(
        f.db.get_files_for_release(release_id)
            .await
            .unwrap()
            .iter()
            .all(|file| file.stored),
        "every file is in the cloud"
    );
    assert_eq!(release_file_ids(f, release_id).await, files, "no row changes");
}

/// A local import stages its files on this device only. Moving it to the
/// cloud queues every file as one move with no row changing; cancelling the
/// move leaves every file here, read from its original; moving it again
/// stores every file.
#[tokio::test]
async fn a_local_import_moves_to_the_cloud_and_cancelling_the_move_keeps_it_here() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let album = two_track_album(&f, "Album One");
    let (id, _) = import_folder(
        &f,
        &album,
        None,
        ImportDestination::Local,
        support::DraftSource::FileTags,
    )
    .await
    .unwrap();
    let files = release_file_ids(&f, &id).await;
    assert_eq!(files.len(), 2);
    assert_on_this_device(&f, &id, &album).await;

    move_to_cloud(&f, &id).await;
    assert_eq!(
        f.db.count_pending_uploads_for_release(&id).await.unwrap(),
        2,
        "every file of the release moves"
    );
    assert_eq!(release_file_ids(&f, &id).await, files, "no row changes");

    f.library_manager.cancel_release_transition(&id).await.unwrap();
    assert_on_this_device(&f, &id, &album).await;
    assert_eq!(release_file_ids(&f, &id).await, files, "no row changes");

    move_to_cloud(&f, &id).await;
    finish_move(&f, &id, &files).await;
}

/// A cloud import stages its files for the cloud, each on this device only
/// until all are stored. Cancelling its upload retires the write's move:
/// every file stays here, read from its original, and the release moves to
/// the cloud again as any local release does.
#[tokio::test]
async fn a_cloud_import_s_upload_cancels_to_a_release_on_this_device() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let album = two_track_album(&f, "Album One");
    let (id, _) = import_folder(
        &f,
        &album,
        None,
        ImportDestination::Remote { pin: false },
        support::DraftSource::FileTags,
    )
    .await
    .unwrap();
    let files = release_file_ids(&f, &id).await;
    assert_eq!(
        f.db.count_pending_uploads_for_release(&id).await.unwrap(),
        2,
        "every file of the import waits to upload"
    );

    f.library_manager.cancel_release_transition(&id).await.unwrap();
    assert_on_this_device(&f, &id, &album).await;
    assert_eq!(release_file_ids(&f, &id).await, files, "no row changes");

    move_to_cloud(&f, &id).await;
    finish_move(&f, &id, &files).await;
}

/// A release made local again is on this device only, its originals the
/// copies coven wrote, and moves back to the cloud as one move.
#[tokio::test]
async fn a_release_made_local_moves_back_to_the_cloud() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let album = two_track_album(&f, "Album One");
    let (id, _) = import_folder(
        &f,
        &album,
        None,
        ImportDestination::Remote { pin: false },
        support::DraftSource::FileTags,
    )
    .await
    .unwrap();
    let files = release_file_ids(&f, &id).await;
    finish_move(&f, &id, &files).await;

    let local = f.temp_path().join("Made Local");
    fs::create_dir_all(&local).unwrap();
    f.library_manager
        .detach_release_audio(&id, &local)
        .await
        .unwrap();
    let copies = local.join(
        f.library_manager
            .file_local_path(&files[0].0)
            .await
            .unwrap()
            .expect("the copy is this device's original")
            .parent()
            .unwrap()
            .strip_prefix(&local)
            .unwrap(),
    );
    assert_on_this_device(&f, &id, &copies).await;

    move_to_cloud(&f, &id).await;
    finish_move(&f, &id, &files).await;
}
