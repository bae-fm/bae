/// A second import over a library that already holds a release: the sequential
/// case, and the refusal that leaves the prior release untouched.
/// 5. Two sequential imports both succeed and produce separate albums.
#[tokio::test]
async fn two_sequential_imports() {
    support::tracing_init();

    let f = ImportFixture::new().await;

    let titles = ["First Album", "Second Album"];
    let mut release_keys = vec![];
    for title in &titles {
        let release = discogs_release(title, &["Track"]);
        release_keys.push(seed_discogs_test_release(f.library_manager.providers(), release));
    }

    let mut release_ids = vec![];
    for (i, title) in titles.iter().enumerate() {
        let _ = title;
        let album_dir = f.temp_path().join(format!("album{}", i + 1));
        fs::create_dir_all(&album_dir).unwrap();
        // Distinct filename per album so the two imports carry different content
        // hashes. The content hash is the relative path + size of each file, and
        // a folder whose content is already imported is refused, so reusing one
        // name would make the second import fail as a re-import.
        let track_name = format!("01 Track {}.flac", i + 1);
        generate_album_files(&album_dir, &[track_name.as_str()]);

        let import_id = uuid::Uuid::new_v4().to_string();
        f.handle
            .send_command(support::folder_import(
                &import_id,
                album_dir,
                support::discogs_release(release_keys[i].clone()),
            ))
            .await
            .unwrap();

        let mut progress_rx = f.handle.subscribe_import(import_id);
        let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;
        release_ids.push(release_id);
    }

    // Both releases exist in DB
    let release1 =
        f.db.find_release_by_id(&release_ids[0])
            .await
            .unwrap()
            .unwrap();
    let release2 =
        f.db.find_release_by_id(&release_ids[1])
            .await
            .unwrap()
            .unwrap();

    // Different albums
    assert_ne!(release1.album_id, release2.album_id);

    let album1 =
        f.db.find_album_by_id(&release1.album_id)
            .await
            .unwrap()
            .unwrap();
    let album2 =
        f.db.find_album_by_id(&release2.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album1.title, "First Album");
    assert_eq!(album2.title, "Second Album");
}

/// A folder that is already in the library is not imported again: the second
/// `ImportCommand` is refused before anything runs, and the prior release —
/// its files, its blob reference — is left exactly as it was. Changing an
/// imported release is the library editor's job, not a re-import's.
#[tokio::test]
async fn an_imported_folder_is_refused_a_second_import() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("already-imported");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileMetadata,
    )
    .await
    .expect("initial import succeeds");
    assert_release_has_external_ref(&f, &prior_release_id).await;
    let content_hash =
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .expect("prior release exists")
            .content_hash
            .clone()
            .unwrap();

    // Whatever the second attempt asks for — a different storage mode, a cover
    // that could never download — it is refused as already imported before
    // any of that is tried.
    for (cover, storage_mode) in [
        (None, StorageMode::Local),
        (
            Some(CoverSelection::Remote(
                bae_core::import::cover_art::RemoteImageSet::original("http://127.0.0.1:9/cover.jpg".to_string()),
                Catalog::MusicBrainz,
            )),
            StorageMode::Local,
        ),
    ] {
        let error = import_folder(
            &f,
            &album_dir,
            cover,
            storage_mode,
            MetadataProvenance::FileMetadata,
        )
        .await
        .expect_err("an imported folder is refused a second import");
        assert!(
            error.contains("already been imported"),
            "unexpected error: {error}"
        );
    }

    assert_release_has_external_ref(&f, &prior_release_id).await;
    assert_eq!(
        f.db.release_ids_for_content_hash(&content_hash)
            .await
            .unwrap(),
        vec![prior_release_id],
        "the prior release still carries the content hash, alone"
    );
}

/// The same refusal for a release that lives in the cloud: the prior release
/// stays exactly as it was, since nothing replaced it.
#[tokio::test]
async fn a_remote_imported_folder_is_refused_a_second_import() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;

    let album_dir = f.temp_path().join("already-imported-remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Remote,
        MetadataProvenance::FileMetadata,
    )
    .await
    .expect("initial remote import queues upload");
    let upload_count = f
        .library_manager
        .drain_uploads_expecting_work()
        .await
        .unwrap();
    assert_eq!(
        upload_count, 1,
        "initial remote import should upload one file"
    );
    let prior_release =
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .expect("prior release exists after upload");
    assert!(prior_release.remote, "prior release should be remote");

    let error = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileMetadata,
    )
    .await
    .expect_err("an imported folder is refused a second import");
    assert!(
        error.contains("already been imported"),
        "unexpected error: {error}"
    );
    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the remote release stays"
    );
}

/// A Remote import records its make-Remote in the write that creates the
/// release, so it needs no cloud connection: with none connected the import
/// commits, its uploads wait in the outbox, and the release is on its way to
/// the cloud rather than rolled back.
#[tokio::test]
async fn a_remote_import_without_a_cloud_connection_queues_its_uploads() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Remote Album",
        "Remote Artist",
        None,
        &[TaggedTrack {
            filename: "01 Remote Track.flac",
            title: "Remote Track",
            track_number: 1,
        }],
    );

    let (release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Remote,
        MetadataProvenance::FileMetadata,
    )
    .await
    .expect("a Remote import commits with no cloud connected");

    let release = f
        .db
        .find_release_by_id(&release_id)
        .await
        .unwrap()
        .expect("the release is committed");
    assert!(!release.remote, "its uploads have not run yet");
    let outbox = f.library_manager.outbox_snapshot().await.unwrap();
    assert!(
        outbox
            .upload_groups
            .iter()
            .any(|group| group.release_id == release_id),
        "its uploads wait in the outbox"
    );
}
