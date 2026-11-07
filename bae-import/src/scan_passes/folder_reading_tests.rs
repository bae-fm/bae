//! Reading one folder again: a folder decision trading its rows at once, a
//! root that moved while it was read, changed folders read alone, and the
//! releases and offers a reading of a folder's discs gives.

use super::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;

/// Every list the live query delivered shows the folder one way or the
/// other: the combined release, or both discs. None shows neither.
fn assert_never_neither(seen: &[Vec<String>], combined: &str, separate: [&str; 2]) {
    for names in seen {
        let shows_combined = names.iter().any(|name| name == combined);
        let shows_separate = separate.iter().all(|disc| names.iter().any(|name| name == disc));
        assert!(
            shows_combined || shows_separate,
            "the list showed the folder as neither reading: {names:?} (all delivered: {seen:?})"
        );
    }
}

/// Keeping a folder's discs separate stores the two disc candidates and
/// drops the combined one in one write — the list never shows the folder
/// with neither — and reads nothing outside the folder's own branch.
#[tokio::test]
async fn keeping_discs_separate_trades_the_combined_row_for_both_discs_at_once() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 3).await;
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()],
        "numbered discs are read as one release"
    );
    let other = fixture.root.join("Other").join("Record");
    let sibling_before = fixture
        .test
        .tables()
        .load_folder_scan_item(&crate::CandidateKey::folder(&other))
        .await
        .unwrap();

    let seen = fixture
        .decide_watching_the_list(
            crate::FolderReleaseDecision::KeepAsSeparateReleases,
            &["Disc 1", "Disc 2"],
        )
        .await;

    assert_never_neither(&seen, "Album", ["Disc 1", "Disc 2"]);
    assert_eq!(
        fixture.stored_paths().await,
        vec![
            "Artist/Album/Disc 1".to_string(),
            "Artist/Album/Disc 2".to_string(),
            "Other/Record".to_string(),
        ]
    );
    assert_eq!(
        fixture
            .test
            .tables()
            .load_folder_release_decisions(&fixture.root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Artist/Album"))
            .map(|reading| (reading.decision, reading.author)),
        Some((
            crate::FolderReleaseDecision::KeepAsSeparateReleases,
            crate::folder_scanner::FolderReleaseDecisionAuthor::User,
        ))
    );
    let read = fixture.listing.take();
    assert!(!read.is_empty());
    assert!(
        read.iter().all(|folder| folder.starts_with(fixture.root.join("Artist"))),
        "only the decided folder's branch is read again: {read:?}"
    );
    assert_eq!(
        fixture
            .test
            .tables()
            .load_folder_scan_item(&crate::CandidateKey::folder(&other))
            .await
            .unwrap(),
        sibling_before,
        "the sibling folder's entry is untouched"
    );
}

/// Combining a folder's parts trades both part rows for the combined one in
/// one write, the same way.
#[tokio::test]
async fn combining_parts_trades_both_rows_for_the_combined_one_at_once() {
    let fixture = DecisionFixture::new(["Live A", "Live B"], 3).await;
    assert_eq!(
        fixture.stored_paths().await,
        vec![
            "Artist/Album/Live A".to_string(),
            "Artist/Album/Live B".to_string(),
            "Other/Record".to_string(),
        ],
        "unnumbered parts are read as separate releases"
    );

    let seen = fixture
        .decide_watching_the_list(crate::FolderReleaseDecision::CombineAsOneRelease, &["Album"])
        .await;

    assert_never_neither(&seen, "Album", ["Live A", "Live B"]);
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()]
    );
    let read = fixture.listing.take();
    assert!(
        read.iter().all(|folder| folder.starts_with(fixture.root.join("Artist"))),
        "only the decided folder's branch is read again: {read:?}"
    );
}

/// A root whose entries were written after the reading began is not the root
/// the reading describes, so the decision fails and stores nothing.
#[tokio::test]
async fn a_folder_reading_refuses_a_root_that_moved_while_it_read() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let before = fixture.stored_paths().await;
    let decisions_before = fixture
        .test
        .tables()
        .load_folder_release_decisions(&fixture.root)
        .await
        .unwrap()
        .get(&crate::RelativeFolder::test("Artist/Album"))
        .cloned();

    // Hold the commit lock, so the reading waits at it with its walk done,
    // and move the root's generation under it the way a pass over the root
    // would.
    let commit = fixture.scan.services.services.folder_state_commit.clone();
    let held = commit.lock("hold for a test").await;
    let reading = {
        let root = fixture.root.clone();
        let key = fixture.album_key();
        let services = fixture.scan.services.clone();
        let cancellation = fixture.scan.cancellation.clone();
        tokio::spawn(async move {
            change_folder_reading(
                &root,
                &(key, crate::FolderReleaseDecision::KeepAsSeparateReleases),
                &services,
                &cancellation,
            )
            .await
        })
    };
    tokio::time::timeout(HANG_GUARD, commit.waiting_reaches(1))
        .await
        .expect("the reading walked and waits at the commit lock");
    fixture.test.tables().begin_folder_scan(&fixture.root, crate::VolumeKind::Local).await.unwrap();
    drop(held);

    let error = reading.await.unwrap().unwrap_err();
    assert!(
        matches!(
            &error,
            crate::ImportError::FolderReread(crate::tables::FolderRereadRefusal::RootMoved(_))
        ),
        "{error:?}"
    );
    assert_eq!(fixture.stored_paths().await, before);
    assert_eq!(
        fixture
            .test
            .tables()
            .load_folder_release_decisions(&fixture.root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Artist/Album"))
            .cloned(),
        decisions_before
    );
}

/// A folder whose contents changed is read again alone, in a write of its
/// own: a new track lands, a folder that went away takes its entries with
/// it, a new folder brings its own, and nothing else under the root is read.
#[tokio::test]
async fn changed_folders_are_read_again_and_nothing_beside_them() {
    let fixture = DecisionFixture::new(["Disc 1", "Disc 2"], 1).await;
    let root = &fixture.root;
    let album = root.join("Artist").join("Album");
    let album_before = fixture
        .test
        .tables()
        .load_folder_scan_item(&crate::CandidateKey::folder(&album))
        .await
        .unwrap();

    write_disc(&root.join("Other").join("Record"), 2);
    write_disc(&root.join("New").join("Record"), 1);
    let folders = ["New", "Other"].into_iter().map(std::ffi::OsString::from).collect();
    read_changed_folders(root, &folders, &fixture.scan.services, &fixture.scan.cancellation).await;

    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "New/Record".to_string(), "Other/Record".to_string(),]
    );
    let other = fixture
        .test
        .tables()
        .load_folder_scan_item(&crate::CandidateKey::folder(&root.join("Other").join("Record")))
        .await
        .unwrap();
    let Some(ScanItem::Valid(other)) = other else {
        panic!("the changed folder is a valid candidate: {other:?}");
    };
    assert_eq!(other.track_count(), 2, "the new track landed");
    let read = fixture.listing.take();
    assert!(
        read.iter()
            .all(|folder| folder.starts_with(root.join("New"))
                || folder.starts_with(root.join("Other"))),
        "only the changed folders are read: {read:?}"
    );
    assert_eq!(
        fixture
            .test
            .tables()
            .load_folder_scan_item(&crate::CandidateKey::folder(&album))
            .await
            .unwrap(),
        album_before,
        "the folder beside them is untouched"
    );

    std::fs::remove_dir_all(root.join("New")).unwrap();
    read_changed_folders(
        root,
        &[std::ffi::OsString::from("New")].into_iter().collect(),
        &fixture.scan.services,
        &fixture.scan.cancellation,
    )
    .await;
    assert_eq!(
        fixture.stored_paths().await,
        vec!["Artist/Album".to_string(), "Other/Record".to_string()],
        "a folder that went away takes its entries with it"
    );
}

/// Each Pending group header with whether it can combine, and each row with
/// whether it can separate.
async fn offers(db: &Store) -> (Vec<(String, bool)>, Vec<(String, bool)>) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let list = tables
        .open_import_list(crate::ImportListView {
            order: crate::ImportListOrder::PathAscending,
            ..crate::ImportListView::default()
        })
        .await
        .unwrap();
    let projection = tables
        .load_import_list(crate::ImportListRequest {
            list,
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        })
        .await
        .unwrap();
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    for item in projection.windows.iter().flat_map(|window| &window.rows) {
        match item {
            crate::ImportListItem::GroupHeader { group, .. } => {
                headers.push((group.name.clone(), group.combinable));
            }
            crate::ImportListItem::Candidate { row, .. } => {
                rows.push((row.display_path.clone(), row.action_basis.separable));
            }
            _ => {}
        }
    }
    (headers, rows)
}

/// A watched root holding `releases`, each a folder of one FLAC track, scanned
/// once.
async fn scanned_root(releases: &[&str]) -> (TestService, TestScan, PathBuf) {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("music");
    for release in releases {
        write_disc(&root.join(release), 1);
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    (test, scan, root)
}

fn folder_key(root: &Path, relative: &str) -> crate::FolderReleaseDecisionKey {
    crate::FolderReleaseDecisionKey {
        watched_folder_path: root.to_path_buf(),
        relative_folder: crate::RelativeFolder::test(relative),
    }
}

/// One album read from two disc folders offers to separate, and its artist's
/// header has nothing to combine.
#[tokio::test]
async fn one_grouped_album_under_an_artist_offers_no_combine() {
    let (test, _scan, _root) = scanned_root(&["Artist/Album/Disc 1", "Artist/Album/Disc 2"]).await;
    let (headers, rows) = offers(&test.db).await;
    assert_eq!(headers, vec![("Artist".to_string(), false)]);
    assert_eq!(rows, vec![("Artist/Album".to_string(), true)]);
}

/// Two albums under an artist's folder offer to combine from the header, and
/// neither separates.
#[tokio::test]
async fn two_albums_under_an_artist_offer_to_combine() {
    let (test, _scan, _root) = scanned_root(&["Artist/Album A", "Artist/Album B"]).await;
    let (headers, rows) = offers(&test.db).await;
    assert_eq!(headers, vec![("Artist".to_string(), true)]);
    assert_eq!(
        rows,
        vec![("Artist/Album A".to_string(), false), ("Artist/Album B".to_string(), false),]
    );
}

/// Disc folders read as one album separate from its row, and once kept apart
/// combine from the header over them.
#[tokio::test]
async fn disc_folders_offer_to_separate_and_kept_apart_offer_to_combine() {
    for (album, header) in [("Album", "Album"), ("Artist/Album", "Artist")] {
        let discs = [format!("{album}/Disc 1"), format!("{album}/Disc 2")];
        let discs: Vec<&str> = discs.iter().map(String::as_str).collect();
        let (test, scan, root) = scanned_root(&discs).await;
        let (_, rows) = offers(&test.db).await;
        assert_eq!(rows, vec![(album.to_string(), true)], "{album}");

        change_folder_reading(
            &root,
            &(folder_key(&root, album), crate::FolderReleaseDecision::KeepAsSeparateReleases),
            &scan.services,
            &scan.cancellation,
        )
        .await
        .unwrap();
        let (headers, rows) = offers(&test.db).await;
        assert_eq!(headers, vec![(header.to_string(), true)], "{album}");
        assert_eq!(
            rows,
            vec![(format!("{album}/Disc 1"), false), (format!("{album}/Disc 2"), false),],
            "{album}"
        );
    }
}

/// A folder read apart and together again keeps its release's key.
#[tokio::test]
async fn a_folders_release_keeps_its_key_across_readings() {
    let (test, scan, root) = scanned_root(&["Album/Disc 1", "Album/Disc 2"]).await;
    let tables = test.tables().clone();
    let grouping_of = || async {
        tables
            .load_folder_release_decisions(&root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Album"))
            .map(|reading| reading.grouping.clone())
            .expect("the scan read the album's discs as one")
    };
    let key = grouping_of().await;
    let stored_release = || async {
        tables
            .load_folder_scan_items(&root)
            .await
            .unwrap()
            .into_iter()
            .filter_map(|item| match item {
                ScanItem::Valid(candidate) => candidate.grouping,
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(stored_release().await, vec![key.clone()]);
    for decision in [
        crate::FolderReleaseDecision::KeepAsSeparateReleases,
        crate::FolderReleaseDecision::CombineAsOneRelease,
    ] {
        change_folder_reading(
            &root,
            &(folder_key(&root, "Album"), decision),
            &scan.services,
            &scan.cancellation,
        )
        .await
        .unwrap();
    }
    assert_eq!(grouping_of().await, key);
    assert_eq!(stored_release().await, vec![key]);
}

/// A folder's own files go to the release it reads as, and each disc is
/// numbered on its own.
#[tokio::test]
async fn a_folder_read_as_one_release_takes_its_own_files_and_numbers_its_discs() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("music");
    for disc in ["Album/Disc 1", "Album/Disc 2"] {
        write_disc(&root.join(disc), 2);
    }
    std::fs::write(root.join("Album/notes.txt"), "notes").unwrap();
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    let release = tables
        .load_folder_scan_items(&root)
        .await
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the album is one release");
    assert!(release.files.release_files().any(|file| file.relative_path == "notes.txt"));
    assert_eq!(
        crate::audio_layout::folder_tracks(&release.files)
            .iter()
            .map(|track| (track.disc, track.number.get()))
            .collect::<Vec<_>>(),
        [(Some(1), 1), (Some(1), 2), (Some(2), 1), (Some(2), 2)]
    );
}
