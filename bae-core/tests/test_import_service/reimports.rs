/// A second import over a library that already holds a release: the sequential
/// case, and repeated copies of the same pressing.
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

/// Repeated imports own distinct release, track and file rows, even when the
/// folder and its pressing are unchanged.
#[tokio::test]
async fn repeated_imports_keep_independent_copies() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let album_dir = f.temp_path().join("repeated-import");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);
    let pressing = seed_discogs_test_release(
        f.library_manager.providers(),
        discogs_release("Album Title", &["Track"]),
    );
    let mut copies = Vec::new();
    for destination in [
        ImportDestination::Local,
        ImportDestination::Local,
        ImportDestination::Remote { pin: false },
        ImportDestination::Remote { pin: false },
    ] {
        let (id, _) = import_folder(
            &f, &album_dir, None, destination,
            support::discogs_release(pressing.clone()),
        ).await.expect("each import creates another copy");
        let release = f.db.find_release_by_id(&id).await.unwrap().unwrap();
        let tracks = f.db.get_tracks_for_release(&id).await.unwrap();
        let files = f.db.get_files_for_release(&id).await.unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(files.len(), 1);
        for (prior, prior_tracks, prior_files) in &copies {
            let prior: &bae_core::db::DbRelease = prior;
            let prior_tracks: &Vec<bae_core::db::DbTrack> = prior_tracks;
            let prior_files: &Vec<bae_core::db::DbFile> = prior_files;
            assert_ne!(release.id, prior.id);
            assert_eq!(release.album_id, prior.album_id);
            assert_eq!(release.content_hash, prior.content_hash);
            assert_ne!(tracks[0].id, prior_tracks[0].id);
            assert_ne!(files[0].id, prior_files[0].id);
            assert_eq!(f.db.get_tracks_for_release(&prior.id).await.unwrap(), *prior_tracks);
            assert_eq!(f.db.get_files_for_release(&prior.id).await.unwrap(), *prior_files);
        }
        if matches!(destination, ImportDestination::Remote { .. }) {
            assert_eq!(f.library_manager.drain_uploads_expecting_work().await.unwrap(), 1);
            assert!(f.db.get_files_for_release(&id).await.unwrap().iter().all(|file| file.stored));
        } else {
            assert_release_has_external_ref(&f, &id).await;
        }
        // Rows as they stand once any upload landed.
        let files = f.db.get_files_for_release(&id).await.unwrap();
        copies.push((release, tracks, files));
    }
    let hash = copies[0].0.content_hash.as_ref().unwrap();
    assert_eq!(f.db.release_ids_for_content_hash(hash).await.unwrap().len(), 4);
    assert!(f.db.is_content_hash_imported(hash).await.unwrap());
    for (release, _, files) in &copies {
        let bytes = f.library_manager.materialize_release_file_for_test(&files[0].id).await;
        assert!(bytes.is_ok(), "copy {} remains readable: {bytes:?}", release.id);
    }
}

/// A Remote import records its upload in the write that creates the
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
        ImportDestination::Remote { pin: false },
        support::DraftSource::FileTags,
    )
    .await
    .expect("a Remote import commits with no cloud connected");

    assert!(f.db.find_release_by_id(&release_id).await.unwrap().is_some());
    // Each row names its file only once its upload completes.
    assert!(f.db.get_files_for_release(&release_id).await.unwrap().iter().all(|file| !file.stored));
    let outbox = f.library_manager.outbox_snapshot().await.unwrap();
    assert!(
        outbox
            .upload_groups
            .iter()
            .any(|group| group.release_id == release_id),
        "its uploads wait in the outbox"
    );
}
