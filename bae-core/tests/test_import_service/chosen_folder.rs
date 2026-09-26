// Choosing a folder to import: it is read before the answer comes back, and
// the answer is what the folder is to the library.

fn album_dir(f: &ImportFixture, relative: &str) -> std::path::PathBuf {
    let dir = f.temp_path().join(relative);
    fs::create_dir_all(&dir).unwrap();
    generate_album_files(&dir, &["01 Track.flac"]);
    dir
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A folder nobody watches is added and read, and the answer names its
/// release — there is no scan still to wait on when it comes back.
#[tokio::test]
async fn a_new_folder_is_added_and_answers_with_its_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let album = album_dir(&f, "Music/Artist/Album");

    let chosen = f
        .handle
        .choose_folder(path_string(&album))
        .await
        .expect("the folder is taken in");

    assert_eq!(
        chosen,
        bae_core::import::ChosenFolder::InImportQueue {
            candidate_keys: vec![path_string(&album)],
        }
    );
    let watched: Vec<String> = f
        .handle
        .watched_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(watched, vec![path_string(&album)]);
}

/// A folder inside a watched one is already covered: no second, overlapping
/// watched folder is added, and the answer names the chosen folder's release
/// alone.
#[tokio::test]
async fn a_folder_inside_a_watched_one_adds_nothing() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let music = f.temp_path().join("Music");
    let album = album_dir(&f, "Music/Artist/Album");
    album_dir(&f, "Music/Artist/Album 2");
    f.handle.choose_folder(path_string(&music)).await.unwrap();

    let chosen = f
        .handle
        .choose_folder(path_string(&album))
        .await
        .expect("the folder is taken in");

    assert_eq!(
        chosen,
        bae_core::import::ChosenFolder::InImportQueue {
            candidate_keys: vec![path_string(&album)],
        }
    );
    let watched: Vec<String> = f
        .handle
        .watched_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(watched, vec![path_string(&music)]);
}

/// A folder whose release is already in the library answers with that album,
/// not the import queue.
#[tokio::test]
async fn an_imported_folder_answers_with_its_album() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let album = f.temp_path().join("Music/Artist/Album");
    fs::create_dir_all(&album).unwrap();
    generate_tagged_album_files(
        &album,
        "Album",
        "Artist",
        None,
        &[TaggedTrack {
            filename: "01 Track.flac",
            title: "Track",
            track_number: 1,
        }],
    );
    let (release_id, _) = import_folder(
        &f,
        &album,
        None,
        StorageMode::Local,
        MetadataProvenance::FileMetadata,
    )
    .await
    .expect("the album imports");
    let album_id =
        f.db.find_release_by_id(&release_id)
            .await
            .unwrap()
            .expect("the release is stored")
            .album_id;

    let chosen = f
        .handle
        .choose_folder(path_string(&album))
        .await
        .expect("the folder is taken in");

    assert_eq!(
        chosen,
        bae_core::import::ChosenFolder::InLibrary { album_id }
    );
}

/// A folder the read cannot open is an error to whoever chose it, not an
/// answer drawn from nothing.
#[tokio::test]
async fn a_folder_that_cannot_be_read_is_an_error() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let missing = f.temp_path().join("Missing");

    let error = f
        .handle
        .choose_folder(path_string(&missing))
        .await
        .expect_err("a folder that is not there cannot be taken in");

    assert!(
        matches!(error, bae_core::import::ImportError::FolderUnread { .. }),
        "unexpected error: {error}"
    );
}

async fn content_hash_of(f: &ImportFixture, key: &str) -> String {
    match f.handle.get_candidate(key).await.unwrap() {
        Some(bae_core::import::ImportCandidateSnapshot::Folder { candidate, .. }) => {
            candidate.files.content_hash()
        }
        other => panic!("{key} is a scanned folder: {other:?}"),
    }
}

/// A folder holding watched folders takes them over: it is watched in their
/// place, and what was decided about their candidates — a skip, a folder read
/// as one release, a pick — carries over, since they are the same files.
#[tokio::test]
async fn a_folder_holding_watched_folders_takes_them_over_and_keeps_their_state() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let music = f.temp_path().join("Music");
    let skipped = album_dir(&f, "Music/Artist/Album");
    let picked = album_dir(&f, "Music/Artist/Album 2");
    album_dir(&f, "Music/Other Artist/Box/Album A");
    album_dir(&f, "Music/Other Artist/Box/Album B");
    let artist = music.join("Artist");
    let other = music.join("Other Artist");
    f.handle.choose_folder(path_string(&artist)).await.unwrap();
    f.handle.choose_folder(path_string(&other)).await.unwrap();

    f.handle
        .set_candidate_skipped(path_string(&skipped), true)
        .await
        .unwrap();
    f.handle
        .select_candidate_metadata_provenance(
            path_string(&picked),
            MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();
    let combined = f
        .handle
        .combine_folder(bae_core::import::FolderReleaseDecisionKey {
            watched_folder_path: path_string(&other),
            relative_folder_path: "Box".to_string(),
        })
        .await
        .unwrap();
    let picked_hash = content_hash_of(&f, &path_string(&picked)).await;

    let chosen = f
        .handle
        .choose_folder(path_string(&music))
        .await
        .expect("the folder takes over the ones inside it");

    let watched: Vec<String> = f
        .handle
        .watched_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(watched, vec![path_string(&music)]);
    assert_eq!(
        chosen,
        bae_core::import::ChosenFolder::InImportQueue {
            candidate_keys: vec![path_string(&picked), combined.clone()],
        },
        "the skipped album stays set aside and the box stays one release"
    );
    match f.handle.get_candidate(&path_string(&skipped)).await.unwrap() {
        Some(bae_core::import::ImportCandidateSnapshot::Folder { skipped, .. }) => {
            assert!(skipped, "the skip carries over")
        }
        other => panic!("the skipped album is still a scanned folder: {other:?}"),
    }
    let state = f
        .library_manager
        .load_import_candidate_state(&picked_hash)
        .await
        .unwrap()
        .expect("the picked album keeps its state");
    assert_eq!(
        state.metadata_provenance,
        Some(MetadataProvenance::FileMetadata),
        "the pick carries over"
    );
}
