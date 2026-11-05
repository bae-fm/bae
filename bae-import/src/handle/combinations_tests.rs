//! Combining folders picked together into one release and separating them
//! again, and the files of a folder picked release: its own audio, and the
//! cover of the folder it sits in when the scan says so. The parts a
//! separation makes are queued for identification only while automatic
//! identification is on.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::tests::import::TestImport;
use std::path::Path;
use std::path::PathBuf;
use tempfile::TempDir;

/// Pending rows the queue holds, by display path, in path order.
async fn pending(handle: &ImportServiceHandle) -> Vec<String> {
    let projection = handle
        .load_list(
            crate::ImportListView {
                order: crate::ImportListOrder::PathAscending,
                ..crate::ImportListView::default()
            },
            [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        )
        .await
        .unwrap();
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, .. } => Some(row.display_path.clone()),
            _ => None,
        })
        .collect()
}

/// Folders picked together leave the queue as one release, and separating it
/// brings them back as they were, drafts and all.
#[tokio::test(flavor = "multi_thread")]
async fn separating_picked_folders_returns_them_as_they_were() {
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (first, first_key, _) = picked_candidate(&tables, &first_root, "Volume A").await;
    let (_, second_key, _) = picked_candidate(&tables, &second_root, "Volume B").await;
    let handle = parts.start().await.unwrap();
    let before = pane(&handle, &first_key).await.draft().unwrap().metadata_draft.clone();
    assert_eq!(pending(&handle).await.len(), 2);

    let key = handle.combine_candidates(vec![first_key.clone(), second_key.clone()]).await.unwrap();
    // The two roots are temporary folders, so which one sorts first — and so
    // plays first and names the release — is the order of their keys.
    let mut members = vec![(first_key.clone(), "Volume A"), (second_key.clone(), "Volume B")];
    members.sort();
    assert_eq!(pending(&handle).await, vec![members[0].1.to_string()]);
    let order: Vec<crate::CandidateKey> = members.into_iter().map(|(key, _)| key).collect();
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::tables::GroupingFacts::Picked { members }) if members == order
    ));

    handle.separate_candidate(&key).await.unwrap();
    assert!(tables.load_grouping(&key).await.unwrap().is_none());
    assert!(handle.get_release_candidate(&key).await.unwrap().is_none());
    assert_eq!(pending(&handle).await.len(), 2);
    assert_eq!(pane(&handle, &first_key).await.draft().unwrap().metadata_draft, before);
    assert_eq!(handle.get_release_candidate(&first_key).await.unwrap().unwrap().files, first.files);
    shut_down(handle).await;
}

/// Picking every release below one folder reads that folder as one release,
/// the way the scan reads a folder of disc parts: the same action, and the
/// same reading the header's "Combine as one release" writes. Separating it
/// keeps the folder's releases apart.
#[tokio::test(flavor = "multi_thread")]
async fn combining_every_release_under_a_folder_reads_that_folder_as_one() {
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("music");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for live in ["Live A", "Live B"] {
        let folder = root.join("Artist").join(live);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = parts.start().await.unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    assert_eq!(
        pending(&handle).await,
        vec!["Artist/Live A".to_string(), "Artist/Live B".to_string()]
    );
    let keys: Vec<crate::CandidateKey> = ["Live A", "Live B"]
        .iter()
        .map(|live| crate::CandidateKey::folder(&root.join("Artist").join(live)))
        .collect();

    let key = handle.combine_candidates(keys.clone()).await.unwrap();
    assert_eq!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::tables::GroupingFacts::Anchored {
            folder: crate::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder: crate::RelativeFolder::test("Artist"),
            },
            decision: crate::FolderReleaseDecision::CombineAsOneRelease,
        })
    );
    assert_eq!(pending(&handle).await, vec!["Artist".to_string()]);
    let release = handle.get_release_candidate(&key).await.unwrap().unwrap();
    assert_eq!(release.files.parts.len(), 2);

    assert_eq!(handle.separate_candidate(&key).await.unwrap(), keys);
    assert_eq!(
        pending(&handle).await,
        vec!["Artist/Live A".to_string(), "Artist/Live B".to_string()]
    );
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::tables::GroupingFacts::Anchored {
            decision: crate::FolderReleaseDecision::KeepAsSeparateReleases,
            ..
        })
    ));
    shut_down(handle).await;
}

/// Separating releases picked together from one folder answers with their
/// keys in the order the list runs them in by path: by their names, numbers
/// read as numbers.
#[tokio::test(flavor = "multi_thread")]
async fn separating_picked_releases_answers_with_their_keys_in_path_order() {
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("music");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for live in ["Live 2", "Live 3", "Live 10"] {
        let folder = root.join("Artist").join(live);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = parts.start().await.unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    let key_of = |live: &str| crate::CandidateKey::folder(&root.join("Artist").join(live));

    // Live 3 stays out, so the pick is not every release under `Artist/`.
    let key = handle.combine_candidates(vec![key_of("Live 10"), key_of("Live 2")]).await.unwrap();
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::tables::GroupingFacts::Picked { .. })
    ));

    assert_eq!(
        handle.separate_candidate(&key).await.unwrap(),
        vec![key_of("Live 2"), key_of("Live 10")]
    );
    shut_down(handle).await;
}

/// `Album` under a fresh watched root holding `discs`, each a folder of one
/// track, beside a cover — and a track of its own when `album_track` — read
/// whole, with the album folder's discs kept as releases of their own.
async fn album_of_discs(
    parts: &TestImport,
    tables: &crate::tables::ImportTables,
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
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = parts.start().await.unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    let reading = tables
        .load_folder_release_decisions(&root)
        .await
        .unwrap()
        .get(&crate::RelativeFolder::test("Album"))
        .cloned()
        .expect("the scan read the album folder");
    if reading.decision == crate::FolderReleaseDecision::CombineAsOneRelease {
        handle.separate_candidate(&reading.grouping).await.unwrap();
    }
    (handle, root.join("Album"))
}

fn key_of(folder: &Path) -> crate::CandidateKey {
    crate::CandidateKey::folder(folder)
}

/// The relative paths of the files the release at `key` holds.
async fn files_of(handle: &ImportServiceHandle, key: &crate::CandidateKey) -> Vec<String> {
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
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let tmp = TempDir::new().unwrap();
    let (handle, album) =
        album_of_discs(&parts, &tables, &tmp, &["Disc 1", "Disc 2", "Disc 3"], false).await;
    assert_eq!(files_of(&handle, &key_of(&album.join("Disc 3"))).await, ["01.flac"]);

    let key = handle
        .combine_candidates(vec![key_of(&album.join("Disc 1")), key_of(&album.join("Disc 2"))])
        .await
        .unwrap();
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::tables::GroupingFacts::Picked { .. })
    ));
    assert_eq!(files_of(&handle, &key).await, ["cover.jpg", "Disc 1/01.flac", "Disc 2/01.flac"]);
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
    let (parts, db, _library) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let tmp = TempDir::new().unwrap();
    let (handle, album) = album_of_discs(&parts, &tables, &tmp, &["Disc 1", "Disc 2"], true).await;
    assert_eq!(files_of(&handle, &key_of(&album)).await, ["00.flac", "cover.jpg"]);

    let discs = handle
        .combine_candidates(vec![key_of(&album.join("Disc 1")), key_of(&album.join("Disc 2"))])
        .await
        .unwrap();
    assert_eq!(files_of(&handle, &discs).await, ["Disc 1/01.flac", "Disc 2/01.flac"]);
    handle.separate_candidate(&discs).await.unwrap();

    let nested = handle
        .combine_candidates(vec![key_of(&album), key_of(&album.join("Disc 1"))])
        .await
        .unwrap();
    assert_eq!(files_of(&handle, &nested).await, ["00.flac", "cover.jpg", "Disc 1/01.flac"]);
    shut_down(handle).await;
}

/// Two releases under `folder`, each with a file of its own, scanned with the
/// setting off and read as one release by the person. Returns the parts and
/// the combined release's key.
async fn combined_parts(fixture: &Fixture, folder: &str) -> (Vec<PathBuf>, crate::CandidateKey) {
    let parts: Vec<PathBuf> = ["Side A", "Side B"]
        .iter()
        .map(|side| {
            let dir = fixture.disc_id_candidate(&format!("{folder}/{side}"));
            std::fs::write(dir.join("notes.txt"), side).unwrap();
            dir
        })
        .collect();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.set_identify_automatically(false).await;
    fixture.scan(2).await;
    let combined = fixture
        .import
        .combine_candidates(parts.iter().map(|dir| crate::CandidateKey::folder(dir)).collect())
        .await
        .unwrap();
    (parts, combined)
}

/// Separating a combined folder finds its parts again: with the setting on
/// each is identified on its own, and with it off none is.
#[tokio::test(flavor = "multi_thread")]
async fn separating_a_folder_queues_its_parts_only_while_the_setting_is_on() {
    for automatic in [true, false] {
        let fixture = Fixture::new(&format!("separate-folder-{automatic}")).await;
        let (parts, combined) = combined_parts(&fixture, "Box").await;
        fixture.set_identify_automatically(automatic).await;

        fixture.import.separate_candidate(&combined).await.unwrap();
        fixture.drain_automatic().await;

        for part in &parts {
            assert_eq!(
                fixture.identified_for(part).await.is_some(),
                automatic,
                "{} with the setting {}",
                part.display(),
                if automatic { "on" } else { "off" }
            );
        }
        // The parts share a disc ID, so one lookup can answer both.
        if !automatic {
            assert_eq!(
                fixture.provider.count_containing("/discid/"),
                0,
                "{:?}",
                fixture.provider.requests()
            );
        }
    }
}

/// Separating releases picked together returns each to the queue as found.
#[tokio::test(flavor = "multi_thread")]
async fn separating_picked_releases_queues_them_only_while_the_setting_is_on() {
    for automatic in [true, false] {
        let fixture = Fixture::new(&format!("separate-picked-{automatic}")).await;
        // A third release beside them keeps the two from being every release
        // in one folder, so they are picked rather than a folder read as one.
        let other = fixture.disc_id_candidate("Other");
        std::fs::write(other.join("notes.txt"), "other").unwrap();
        let picked: Vec<PathBuf> = ["Album A", "Album B"]
            .iter()
            .map(|name| {
                let dir = fixture.disc_id_candidate(name);
                std::fs::write(dir.join("notes.txt"), name).unwrap();
                dir
            })
            .collect();
        fixture.provider.route("/discid/", 200, "{}");
        fixture.set_identify_automatically(false).await;
        fixture.scan(3).await;
        let combined = fixture
            .import
            .combine_candidates(picked.iter().map(|dir| crate::CandidateKey::folder(dir)).collect())
            .await
            .unwrap();
        assert!(combined.as_bytes().starts_with(b"grouping:"), "{combined} is a picked grouping");
        fixture.set_identify_automatically(automatic).await;

        fixture.import.separate_candidate(&combined).await.unwrap();
        fixture.drain_automatic().await;

        for dir in &picked {
            assert_eq!(fixture.identified_for(dir).await.is_some(), automatic);
        }
        assert!(fixture.identified_for(&other).await.is_none());
        // The parts share a disc ID, so one lookup can answer both.
        if !automatic {
            assert_eq!(
                fixture.provider.count_containing("/discid/"),
                0,
                "{:?}",
                fixture.provider.requests()
            );
        }
    }
}
