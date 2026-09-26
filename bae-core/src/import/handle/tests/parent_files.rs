//! The files of the folder picked releases sit in, read from disk end to end:
//! the scan says them, and the release the picked releases make reads them.

use super::*;

/// `Album` under a fresh watched root holding `discs`, each a folder of one
/// track, beside a cover — and a track of its own when `album_track` — read
/// whole, with the album folder's discs kept as releases of their own.
async fn album_of_discs(
    manager: &LibraryManager,
    tmp: &TempDir,
    discs: &[&str],
    album_track: bool,
) -> (ImportServiceHandle, PathBuf) {
    let root = tmp.path().join("music");
    let album = root.join("Album");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for disc in discs {
        std::fs::create_dir_all(album.join(disc)).unwrap();
        std::fs::copy(&fixture, album.join(disc).join("01.flac")).unwrap();
    }
    if album_track {
        std::fs::copy(&fixture, album.join("00.flac")).unwrap();
    }
    std::fs::write(album.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    let root = crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy())
        .unwrap();
    manager.add_watched_import_folder(&root).await.unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    let reading = handle
        .library_manager
        .load_folder_release_decisions(&root)
        .await
        .unwrap()
        .get("Album")
        .cloned()
        .expect("the scan read the album folder");
    if reading.decision == crate::import::FolderReleaseDecision::CombineAsOneRelease {
        handle.separate_candidate(&reading.grouping).await.unwrap();
    }
    (handle, PathBuf::from(root).join("Album"))
}

fn key_of(folder: &Path) -> String {
    folder.to_string_lossy().into_owned()
}

/// The relative paths of the files the release at `key` holds.
async fn files_of(handle: &ImportServiceHandle, key: &str) -> Vec<String> {
    handle
        .get_release_candidate(key)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("{key} is a release to work on"))
        .files
        .files
        .iter()
        .map(|entry| entry.file.relative_path.clone())
        .collect()
}

/// Two discs picked out of three in the album folder make a release that
/// holds the album's cover, which the disc left out does not; separated, the
/// discs hold only their own files again.
#[tokio::test(flavor = "multi_thread")]
async fn two_discs_of_three_take_the_album_folders_cover() {
    let (manager, _library) = setup_test_manager().await;
    let tmp = TempDir::new().unwrap();
    let (handle, album) =
        album_of_discs(&manager, &tmp, &["Disc 1", "Disc 2", "Disc 3"], false).await;
    assert_eq!(files_of(&handle, &key_of(&album.join("Disc 3"))).await, ["01.flac"]);

    let key = handle
        .combine_candidates(vec![
            key_of(&album.join("Disc 1")),
            key_of(&album.join("Disc 2")),
        ])
        .await
        .unwrap();
    assert!(matches!(
        handle.library_manager.load_grouping(&key).await.unwrap(),
        Some(crate::db::GroupingFacts::Picked { .. })
    ));
    assert_eq!(
        files_of(&handle, &key).await,
        ["cover.jpg", "Disc 1/01.flac", "Disc 2/01.flac"]
    );
    assert_eq!(files_of(&handle, &key_of(&album.join("Disc 3"))).await, ["01.flac"]);

    handle.separate_candidate(&key).await.unwrap();
    assert_eq!(files_of(&handle, &key_of(&album.join("Disc 1"))).await, ["01.flac"]);
    shut_down(handle).await;
}

/// An album folder with a track of its own is a release, and its cover is
/// that release's: discs picked below it do not take it, and the album picked
/// with one of its discs holds it once, as its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_folder_that_is_a_release_keeps_its_cover() {
    let (manager, _library) = setup_test_manager().await;
    let tmp = TempDir::new().unwrap();
    let (handle, album) = album_of_discs(&manager, &tmp, &["Disc 1", "Disc 2"], true).await;
    assert_eq!(
        files_of(&handle, &key_of(&album)).await,
        ["00.flac", "cover.jpg"]
    );

    let discs = handle
        .combine_candidates(vec![
            key_of(&album.join("Disc 1")),
            key_of(&album.join("Disc 2")),
        ])
        .await
        .unwrap();
    assert_eq!(
        files_of(&handle, &discs).await,
        ["Disc 1/01.flac", "Disc 2/01.flac"]
    );
    handle.separate_candidate(&discs).await.unwrap();

    let nested = handle
        .combine_candidates(vec![key_of(&album), key_of(&album.join("Disc 1"))])
        .await
        .unwrap();
    assert_eq!(
        files_of(&handle, &nested).await,
        ["00.flac", "cover.jpg", "Disc 1/01.flac"]
    );
    shut_down(handle).await;
}
