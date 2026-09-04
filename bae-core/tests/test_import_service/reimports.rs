/// A second import over a library that already holds a release: the sequential
/// case, the failure paths that must leave the prior release untouched, and the
/// successful replacement.
/// 5. Two sequential imports both succeed and produce separate albums.
#[tokio::test]
async fn two_sequential_imports() {
    support::tracing_init();

    let titles = ["First Album", "Second Album"];
    let mut release_keys = vec![];
    for title in &titles {
        let release = discogs_release(title, &["Track"]);
        release_keys.push(seed_discogs_test_release(release));
    }
    let f = ImportFixture::new().await;

    let mut release_ids = vec![];
    for (i, title) in titles.iter().enumerate() {
        let _ = title;
        let album_dir = f.temp_path().join(format!("album{}", i + 1));
        fs::create_dir_all(&album_dir).unwrap();
        // Distinct filename per album so the two imports carry different content
        // hashes. The content hash is the relative path + size of each file, and
        // re-importing the same content overwrites the prior release, so reusing
        // one name would make the second import delete the first.
        let track_name = format!("01 Track {}.flac", i + 1);
        generate_album_files(&album_dir, &[track_name.as_str()]);

        let import_id = uuid::Uuid::new_v4().to_string();
        f.handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir,
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                    source: MetadataSource::Discogs,
                release_id: release_keys[i].clone(),
                    partners: vec![],
                }),
                user_edit: None,
            })
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

#[tokio::test]
async fn reimport_cover_download_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("cover-failure-reimport");
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
        MetadataProvenance::FileTags,
    )
    .await
    .expect("initial import succeeds");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    let result = import_folder(
        &f,
        &album_dir,
        Some(CoverSelection::Remote(
            "http://127.0.0.1:9/cover.jpg".to_string(),
            MetadataSource::MusicBrainz,
        )),
        StorageMode::Local,
        MetadataProvenance::FileTags,
    )
    .await;

    assert!(result.is_err(), "cover download should fail");
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn reimport_decode_verification_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.set_decode_verification(false);

    let album_dir = f.temp_path().join("decode-failure-reimport");
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
    truncate_flac_body(&album_dir.join("01 Track Title.flac"));

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileTags,
    )
    .await
    .expect("initial import succeeds while decode verification is disabled");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    f.set_decode_verification(true);
    let result = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileTags,
    )
    .await;

    let error = result.expect_err("decode verification should fail");
    assert!(
        error.contains("decode verification failed"),
        "unexpected error: {error}"
    );
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn successful_reimport_replaces_prior_release_once() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;

    let album_dir = f.temp_path().join("successful-reimport");
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
        MetadataProvenance::FileTags,
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
    let content_hash = prior_release.content_hash.clone().unwrap();

    let (replacement_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileTags,
    )
    .await
    .expect("re-import succeeds");

    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_none(),
        "prior release should be replaced"
    );
    let release_ids =
        f.db.release_ids_for_content_hash(&content_hash)
            .await
            .unwrap();
    assert_eq!(
        release_ids,
        vec![replacement_release_id],
        "one release should carry the re-imported content hash"
    );
    assert_eq!(
        f.db.queued_delete_count_for_test().await.unwrap(),
        1,
        "replacing the prior remote release should queue its cloud blob for deletion"
    );
}

/// The remote-transition rollback: the mirror of the local unit test
/// `failed_import_before_finalize_leaves_only_import_audit_row`, but one stage
/// later. The release is finalized (status Importing), then the cloud
/// transition fails and `run_import` calls `fail_import_and_delete_release`. A
/// Remote import with no sync provider connected fails at exactly that point
/// (`coven_make_remote` returns `SyncNotReady`) — the honest injection for a
/// post-finalize transition failure, since the upload itself is deferred to the
/// drain and never runs synchronously. The rollback must delete the
/// just-finalized release and its album, mark the import Failed with its release
/// link cleared, and leave a pre-existing release untouched.
#[tokio::test]
async fn remote_transition_failure_rolls_back_finalized_release() {
    support::tracing_init();
    // No cloud/sync connected, so the make-Remote transition fails.
    let f = ImportFixture::new().await;

    // A prior local release already in the library; the failed remote import
    // below must not touch it.
    let prior_dir = f.temp_path().join("prior");
    fs::create_dir_all(&prior_dir).unwrap();
    generate_tagged_album_files(
        &prior_dir,
        "Prior Album",
        "Prior Artist",
        None,
        &[TaggedTrack {
            filename: "01 Prior Track.flac",
            title: "Prior Track",
            track_number: 1,
        }],
    );
    let (prior_release_id, _) = import_folder(
        &f,
        &prior_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::FileTags,
    )
    .await
    .expect("prior local import succeeds");

    // The remote import: finalize commits the release (status Importing), then
    // coven_make_remote fails because sync was never connected.
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

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id.clone());
    let error = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect_err("remote transition without a sync provider fails");
    assert!(error.contains("cloud upload"), "unexpected error: {error}");

    // The rollback deleted the finalized remote release, its album, and the
    // artist row that finalize inserted for it; only the prior release, album,
    // and artist remain. The remote import's artist is referenced by nothing
    // else, so leaving it behind would orphan a row on every failed remote
    // import.
    let (release_count, album_count, artist_count) =
        f.db.library_row_counts_for_test().await.unwrap();
    assert_eq!(release_count, 1, "only the prior release remains");
    assert_eq!(album_count, 1, "only the prior album remains");
    assert_eq!(
        artist_count, 1,
        "only the prior artist remains; the rolled-back import's artist row is gone",
    );
    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the prior release is untouched by the failed remote import",
    );
}
