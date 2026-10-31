// Files whose names aren't UTF-8. Only Linux takes such a name: macOS and
// Windows store names as Unicode.

/// The Latin-1 spelling of `Café` followed by `extension`: a name that isn't
/// UTF-8.
#[cfg(target_os = "linux")]
fn latin1_name(extension: &str) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut name = b"Caf\xE9.".to_vec();
    name.extend_from_slice(extension.as_bytes());
    std::ffi::OsString::from_vec(name)
}

/// `Collection/Album` holding one tagged track named with the Latin-1 bytes
/// of `Café.flac`; the folder and the track's path.
#[cfg(target_os = "linux")]
fn album_with_a_latin1_name(collection: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let album = collection.join("Album");
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "Track.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );
    let track = album.join(latin1_name("flac"));
    fs::rename(album.join("Track.flac"), &track).unwrap();
    (album, track)
}

/// Watch `collection` and return the releases its first scan lists, once the
/// scan has finished.
#[cfg(target_os = "linux")]
async fn scanned_releases(
    f: &ImportFixture,
    collection: &Path,
) -> Vec<bae_core::import::FolderCandidate> {
    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let mut releases = Vec::new();
    support::next_matching(&mut scan_rx, HANG_GUARD, |event| match event {
        ScanEvent::FolderCandidate { candidate, .. } => {
            releases.push(candidate);
            None
        }
        ScanEvent::Finished => Some(()),
        _ => None,
    })
    .await
    .expect("the scan finishes");
    releases
}

/// A track whose name isn't UTF-8 doesn't fail its folder's scan: the release
/// is listed with the track under its escaped name, imports under that name,
/// and the library records the track's own path, which opens it.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_is_scanned_and_imported_under_its_escaped_name() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let (album, track) = album_with_a_latin1_name(&collection);

    let releases = scanned_releases(&f, &collection).await;

    let [release] = releases.as_slice() else {
        panic!("the scan lists the album: {releases:?}");
    };
    let shown: Vec<(&str, &str)> = release
        .files
        .files
        .iter()
        .map(|entry| (entry.file.relative_path.as_str(), entry.file.file_name.as_str()))
        .collect();
    assert_eq!(shown, [(r"Caf\xE9.flac", r"Caf\xE9.flac")]);
    assert_eq!(release.files.files[0].file.path, track);

    let key = album.to_string_lossy().into_owned();
    f.handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let import_id = f.handle.start_import(&key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect("the import completes");

    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    let [file] = files.as_slice() else {
        panic!("the release holds the track: {files:?}");
    };
    assert_eq!(file.original_filename, r"Caf\xE9.flac");
    let original = f
        .db
        .external_file(&file.id)
        .await
        .unwrap()
        .expect("the track's original is recorded");
    assert_eq!(original.path, track);
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.source_folder_name.as_deref(), Some("Album"));
}

/// A track whose name isn't UTF-8 uploads: coven reads it by its own path.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_uploads() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let collection = f.temp_path().join("Collection");
    let (album, _track) = album_with_a_latin1_name(&collection);
    let releases = scanned_releases(&f, &collection).await;
    assert_eq!(releases.len(), 1, "the scan lists the album: {releases:?}");
    let key = album.to_string_lossy().into_owned();
    f.handle.select_candidate_file_tags(key.clone()).await.unwrap();

    f.library_manager.set_import_to_cloud(true).await.unwrap();
    let import_id = f.handle.start_import(&key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let outcome = support::wait_for_import_end(&mut progress_rx).await;
    let bae_core::import::ImportProgress::RemoteUploadQueued { id, .. } = outcome else {
        panic!("the cloud choice queues the release's upload: {outcome:?}");
    };

    assert_eq!(
        f.library_manager.drain_uploads_expecting_work().await.unwrap(),
        1,
        "the release's one track uploads"
    );
    let files = f.db.get_files_for_release(&id).await.unwrap();
    assert!(
        files
            .iter()
            .all(|file| file.stored && file.original_filename == r"Caf\xE9.flac"),
        "the track is stored under its escaped name: {files:?}"
    );
}

/// A track spelled `Caf\xE9.flac` and one whose name is the Latin-1 bytes of
/// `Café.flac` show one name, which a release can't hold twice: the folder is
/// listed under Skipped, naming it.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn two_tracks_showing_one_name_are_skipped_with_that_name() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let album = collection.join("Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &[r"Caf\xE9.flac", "Track.flac"]);
    fs::rename(album.join("Track.flac"), album.join(latin1_name("flac"))).unwrap();

    assert!(scanned_releases(&f, &collection).await.is_empty());

    let expected = bae_core::import::InvalidReason::SameName {
        name: r"Caf\xE9.flac".to_string(),
    };
    wait_for_tab(
        &f,
        "the folder under Skipped",
        bae_core::import::TriageTab::Skipped,
        |snapshot| {
            snapshot
                .windows
                .iter()
                .flat_map(|window| &window.items)
                .any(|item| match item {
                    bae_core::import::ImportListItem::Invalid { candidate, .. } => {
                        candidate.path == album
                            && candidate.reason() == expected
                            && candidate.files.len() == 2
                    }
                    _ => false,
                })
        },
    )
    .await;
}
