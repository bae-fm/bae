use super::*;

/// The Latin-1 spelling of `Café` followed by `rest`: a name that isn't
/// UTF-8.
fn latin1_name(rest: &str) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut name = b"Caf\xE9".to_vec();
    name.extend_from_slice(rest.as_bytes());
    std::ffi::OsString::from_vec(name)
}

/// `Collection/Album` holding one tagged track named with the Latin-1 bytes
/// of `Café.flac`; the folder and the track's path.
fn album_with_a_latin1_name(collection: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let album = collection.join("Album");
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack { filename: "Track.flac", title: "Track Title", track_number: 1 }],
    );
    let track = album.join(latin1_name(".flac"));
    fs::rename(album.join("Track.flac"), &track).unwrap();
    (album, track)
}

/// Watch `collection` and return the releases its first scan lists, once the
/// scan has finished.
async fn scanned_releases(
    f: &ImportFixture,
    collection: &Path,
) -> Vec<bae_import::FolderCandidate> {
    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle.add_watched_folder(collection.to_path_buf()).await.unwrap();
    let mut releases = Vec::new();
    bae_util::test_channels::next_matching(&mut scan_rx, HANG_GUARD, |event| match event {
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

/// Import the release the scan listed for `album`, from its files' tags, and
/// return its id once the import completes.
async fn import_scanned(f: &ImportFixture, album: &Path) -> String {
    let key = bae_import::CandidateKey::folder(album);
    f.handle.select_candidate_file_tags(key.clone()).await.unwrap();
    let import_id = f.handle.start_import(&key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = bae_import::test_imports::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect("the import completes");
    release_id
}

/// `Collection/Café`, its name Latin-1, holding one tagged track; the folder.
fn latin1_album(collection: &Path) -> std::path::PathBuf {
    let album = collection.join(latin1_name(""));
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack { filename: "Track.flac", title: "Track Title", track_number: 1 }],
    );
    album
}

/// A folder whose name isn't UTF-8 is listed under its escaped name, keyed
/// by its own path, and imports: the library records its tracks' own paths
/// and its escaped name as the release's folder name.
#[tokio::test]
async fn a_folder_whose_name_is_not_utf8_is_scanned_and_imported_under_its_escaped_name() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let album = latin1_album(&collection);

    let releases = scanned_releases(&f, &collection).await;

    let [release] = releases.as_slice() else {
        panic!("the scan lists the folder: {releases:?}");
    };
    assert_eq!(release.path, album);
    assert_eq!(release.name, r"Caf\xE9");
    assert_eq!(release.key(), bae_import::CandidateKey::folder(&album));

    let release_id = import_scanned(&f, &album).await;

    let files = f.library.database().get_files_for_release(&release_id).await.unwrap();
    let [file] = files.as_slice() else {
        panic!("the release holds the track: {files:?}");
    };
    assert_eq!(file.original_filename, "Track.flac");
    let original = f
        .library
        .database()
        .external_file(&file.id)
        .await
        .unwrap()
        .expect("the track's original is recorded");
    assert_eq!(original.path, album.join("Track.flac"));
    let release = f.library.database().find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.source_folder_name.as_deref(), Some(r"Caf\xE9"));
}

/// Skipping a folder whose name isn't UTF-8 is stored under its own bytes
/// below the watched folder, so the skip names that folder and no other.
#[tokio::test]
async fn a_folder_whose_name_is_not_utf8_is_skipped_by_its_own_path() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let album = latin1_album(&collection);
    let releases = scanned_releases(&f, &collection).await;
    assert_eq!(releases.len(), 1, "the scan lists the folder: {releases:?}");
    let key = bae_import::CandidateKey::folder(&album);

    f.handle.set_candidate_skipped(key.clone(), true).await.unwrap();

    let skipped = bae_import::tables::ImportTables::new(f.library.database().clone())
        .load_skipped_import_candidates(&collection)
        .await
        .unwrap();
    let expected = bae_import::RelativeFolder::of(Path::new(&latin1_name(""))).unwrap();
    assert_eq!(skipped, std::collections::HashSet::from([expected]));
    match f.handle.get_candidate(&key).await.unwrap() {
        Some(bae_import::ImportCandidateSnapshot::Folder { skipped, .. }) => {
            assert!(skipped, "the folder reads as skipped");
        }
        other => panic!("{key} is a scanned folder: {other:?}"),
    }
}

/// The two releases in a folder whose name isn't UTF-8 combine into one,
/// read from that folder: the decision is kept under its bytes.
#[tokio::test]
async fn the_releases_in_a_folder_whose_name_is_not_utf8_combine_into_it() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let boxed = collection.join(latin1_name(""));
    let albums = [boxed.join("Album A"), boxed.join("Album B")];
    for album in &albums {
        fs::create_dir_all(album).unwrap();
        generate_album_files(album, &["01 Track.flac"]);
    }
    let releases = scanned_releases(&f, &collection).await;
    assert_eq!(releases.len(), 2, "the scan lists each album: {releases:?}");

    let combined = f
        .handle
        .combine_candidates(
            albums.iter().map(|album| bae_import::CandidateKey::folder(album)).collect(),
        )
        .await
        .expect("the albums combine");

    assert_eq!(f.handle.candidate_source_folders(&combined).await.unwrap(), albums);
    let reading = bae_import::tables::ImportTables::new(f.library.database().clone())
        .folder_reading_grouping(&bae_import::FolderReleaseDecisionKey {
            watched_folder_path: collection.clone(),
            relative_folder: bae_import::RelativeFolder::of(Path::new(&latin1_name(""))).unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(reading, Some(combined), "the folder is read as the combined release");
}

/// A track whose name isn't UTF-8 doesn't fail its folder's scan: the release
/// is listed with the track under its escaped name, imports under that name,
/// and the library records the track's own path, which opens it.
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_is_scanned_and_imported_under_its_escaped_name() {
    bae_util::test_logs::install();
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

    let release_id = import_scanned(&f, &album).await;

    let files = f.library.database().get_files_for_release(&release_id).await.unwrap();
    let [file] = files.as_slice() else {
        panic!("the release holds the track: {files:?}");
    };
    assert_eq!(file.original_filename, r"Caf\xE9.flac");
    let original = f
        .library
        .database()
        .external_file(&file.id)
        .await
        .unwrap()
        .expect("the track's original is recorded");
    assert_eq!(original.path, track);
    let release = f.library.database().find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.source_folder_name.as_deref(), Some("Album"));
}

/// A track whose name isn't UTF-8 exports under the name coven writes for its
/// escaped name, `Caf＼xE9.flac`: a stored name is a label, written the same
/// way on every platform, so the export doesn't recreate the Latin-1 bytes.
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_exports_under_the_name_coven_writes() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let (album, track) = album_with_a_latin1_name(&collection);
    let releases = scanned_releases(&f, &collection).await;
    assert_eq!(releases.len(), 1, "the scan lists the album: {releases:?}");
    let release_id = import_scanned(&f, &album).await;

    let target = f.temp_path().join("Export");
    fs::create_dir_all(&target).unwrap();
    f.library
        .export_release(&release_id, &target, bae_transfers::OutputKind::Export)
        .await
        .expect("the release exports");

    let exported = target.join("Album").join("Caf＼xE9.flac");
    assert_eq!(fs::read(&exported).unwrap(), fs::read(&track).unwrap());
    let mut names: Vec<std::ffi::OsString> = fs::read_dir(target.join("Album"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, [".bae-output", "Caf＼xE9.flac"]);
}

/// A track whose name isn't UTF-8 uploads: coven reads it by its own path.
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_uploads() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;
    let collection = f.temp_path().join("Collection");
    let (album, _track) = album_with_a_latin1_name(&collection);
    let releases = scanned_releases(&f, &collection).await;
    assert_eq!(releases.len(), 1, "the scan lists the album: {releases:?}");
    let key = bae_import::CandidateKey::folder(&album);
    f.handle.select_candidate_file_tags(key.clone()).await.unwrap();

    f.library.set_import_to_cloud(true).await.unwrap();
    let import_id = f.handle.start_import(&key).await.unwrap().expect_queued();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let outcome = bae_import::test_imports::wait_for_import_end(&mut progress_rx).await;
    let bae_import::ImportProgress::RemoteUploadQueued { id, .. } = outcome else {
        panic!("the cloud choice queues the release's upload: {outcome:?}");
    };

    assert_eq!(
        f.library.drain_uploads_expecting_work().await.unwrap(),
        1,
        "the release's one track uploads"
    );
    let files = f.library.database().get_files_for_release(&id).await.unwrap();
    assert!(
        files.iter().all(|file| file.stored && file.original_filename == r"Caf\xE9.flac"),
        "the track is stored under its escaped name: {files:?}"
    );
}

/// A track spelled `Caf\xE9.flac` and one whose name is the Latin-1 bytes of
/// `Café.flac` show one name, which a release can't hold twice: the folder is
/// listed under Skipped, naming it.
#[tokio::test]
async fn two_tracks_showing_one_name_are_skipped_with_that_name() {
    bae_util::test_logs::install();
    let f = ImportFixture::new().await;
    let collection = f.temp_path().join("Collection");
    let album = collection.join("Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &[r"Caf\xE9.flac", "Track.flac"]);
    fs::rename(album.join("Track.flac"), album.join(latin1_name(".flac"))).unwrap();

    assert!(scanned_releases(&f, &collection).await.is_empty());

    let expected = bae_import::InvalidReason::SameName { name: r"Caf\xE9.flac".to_string() };
    wait_for_tab(&f, "the folder under Skipped", bae_import::TriageTab::Skipped, |snapshot| {
        snapshot.windows.iter().flat_map(|window| &window.items).any(|item| match item {
            bae_import::ImportListItem::Invalid { candidate, .. } => {
                candidate.path == album
                    && candidate.reason() == expected
                    && candidate.files.len() == 2
            }
            _ => false,
        })
    })
    .await;
}
