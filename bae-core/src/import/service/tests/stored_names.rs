/// Windows takes a name holding an unpaired UTF-16 surrogate, which isn't
/// Unicode, so the release stores it as escaped text (`Caf\xED\xA0\x80.flac`),
/// whose `\` Windows reads as a separator. A stored name is a label: the track
/// imports under that text, and its export writes the name coven writes for
/// it, the same on every platform.
#[cfg(windows)]
#[tokio::test]
async fn a_track_named_with_an_unpaired_surrogate_imports_and_exports() {
    use std::os::windows::ffi::OsStringExt;

    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    let folder = root.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    let name: Vec<u16> = "Caf"
        .encode_utf16()
        .chain([0xD800])
        .chain(".flac".encode_utf16())
        .collect();
    std::fs::write(folder.join(std::ffi::OsString::from_wide(&name)), flac()).unwrap();
    let key = crate::import::CandidateKey::folder(&folder);
    test.service
        .library_manager
        .add_watched_import_folder(&root)
        .await
        .unwrap();
    let (scan, _) = test.scan();
    scan.rescan(&root).await.unwrap();
    let candidate = test
        .service
        .library_manager
        .load_release_candidate(&key)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let hash = candidate.files.content_hash();
    let revision = prepare_named_candidate(
        &test.service,
        &test.preparations,
        &hash,
        &root,
        &key,
        "Album Title",
    )
    .await;
    let reading = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap()
        .snapshot
        .unwrap();

    test.service.queue_import_for_test(&key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            key,
            candidate.source(),
            super::ImportExpectation {
                candidate: crate::import::CandidateAsRead {
                    content_hash: hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                file_tag_snapshot: Some(reading),
            },
            ImportDestination::Local,
        )
        .await
        .expect("the track imports");

    let manager = &test.service.library_manager;
    let albums = manager.get_albums(&[]).await.unwrap();
    let [album] = albums.as_slice() else {
        panic!("one album: {albums:?}");
    };
    let releases = manager.get_releases_for_album(&album.id).await.unwrap();
    let [release] = releases.as_slice() else {
        panic!("one release: {releases:?}");
    };
    let files = manager.get_files_for_release(&release.id).await.unwrap();
    let names: Vec<&str> = files
        .iter()
        .map(|file| file.original_filename.as_str())
        .collect();
    assert_eq!(names, [r"Caf\xED\xA0\x80.flac"]);

    let target = test.temp.path().join("export");
    manager
        .export_release(&release.id, &target, crate::library::OutputKind::Export)
        .await
        .expect("the release exports");
    assert_eq!(
        std::fs::read(target.join("Album").join("Caf＼xED＼xA0＼x80.flac")).unwrap(),
        flac()
    );
}
