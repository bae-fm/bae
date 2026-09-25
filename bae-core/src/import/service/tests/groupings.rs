/// The list as the Pending tab shows it: each group header with whether it
/// offers to read its folder's releases as one, and each release row with
/// whether it offers to read itself as the folders it is made of.
async fn offers(manager: &LibraryManager) -> (Vec<(String, bool)>, Vec<(String, bool)>) {
    let projection = manager
        .load_import_list(crate::import::ImportListRequest {
            view: crate::import::ImportListView {
                order: crate::import::ImportListOrder::PathAscending,
                ..crate::import::ImportListView::default()
            },
            windows: [crate::library::LibraryPageWindow {
                offset: 0,
                limit: 50,
            }]
            .into_iter()
            .collect(),
            upload_standing: Default::default(),
        })
        .await
        .unwrap();
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    for item in projection.windows.iter().flat_map(|window| &window.items) {
        match item {
            crate::import::ImportListItem::GroupHeader { group, .. } => {
                headers.push((group.name.clone(), group.combinable));
            }
            crate::import::ImportListItem::Candidate { row, .. } => {
                rows.push((row.display_path.clone(), row.separable));
            }
            _ => {}
        }
    }
    (headers, rows)
}

/// A watched root holding `releases` — each a folder of one FLAC track —
/// read whole once, as when it was added.
async fn scanned_root(releases: &[&str]) -> (TestService, TestScan, PathBuf) {
    let test = setup_import_service().await;
    let root = test.temp.path().join("music");
    for release in releases {
        write_disc(&root.join(release), 1);
    }
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy()).unwrap(),
    );
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    (test, scan, root)
}

fn folder_key(root: &Path, relative: &str) -> crate::import::FolderReleaseDecisionKey {
    crate::import::FolderReleaseDecisionKey {
        watched_folder_path: root.to_string_lossy().into_owned(),
        relative_folder_path: relative.to_string(),
    }
}

/// One album read from two disc folders under an artist's folder is one
/// release there: the artist's header has nothing to combine, and the album's
/// row offers to read its discs apart.
#[tokio::test]
async fn one_grouped_album_under_an_artist_offers_no_combine() {
    let (test, _scan, _root) =
        scanned_root(&["Artist/Album/Disc 1", "Artist/Album/Disc 2"]).await;
    let (headers, rows) = offers(&test.service.library_manager).await;
    assert_eq!(headers, vec![("Artist".to_string(), false)]);
    assert_eq!(rows, vec![("Artist/Album".to_string(), true)]);
}

/// Two albums under an artist's folder: the artist's header offers to read
/// them as one, and neither album offers anything to separate.
#[tokio::test]
async fn two_albums_under_an_artist_offer_to_combine() {
    let (test, _scan, _root) = scanned_root(&["Artist/Album A", "Artist/Album B"]).await;
    let (headers, rows) = offers(&test.service.library_manager).await;
    assert_eq!(headers, vec![("Artist".to_string(), true)]);
    assert_eq!(
        rows,
        vec![
            ("Artist/Album A".to_string(), false),
            ("Artist/Album B".to_string(), false),
        ]
    );
}

/// Disc folders read as one album offer to be read apart, from the album's
/// row; kept apart, they offer to be read as one again, from the header over
/// them — at the top of the root and under an artist alike.
#[tokio::test]
async fn disc_folders_offer_to_separate_and_kept_apart_offer_to_combine() {
    for (album, header) in [("Album", "Album"), ("Artist/Album", "Artist")] {
        let discs = [format!("{album}/Disc 1"), format!("{album}/Disc 2")];
        let discs: Vec<&str> = discs.iter().map(String::as_str).collect();
        let (test, scan, root) = scanned_root(&discs).await;
        let (_, rows) = offers(&test.service.library_manager).await;
        assert_eq!(rows, vec![(album.to_string(), true)], "{album}");

        ImportService::change_folder_reading(
            &root,
            &(
                folder_key(&root, album),
                crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
            ),
            &scan.services,
            &scan.cancellation,
        )
        .await
        .unwrap();
        let (headers, rows) = offers(&test.service.library_manager).await;
        assert_eq!(headers, vec![(header.to_string(), true)], "{album}");
        assert_eq!(
            rows,
            vec![
                (format!("{album}/Disc 1"), false),
                (format!("{album}/Disc 2"), false),
            ],
            "{album}"
        );
    }
}

/// Reading a folder as one release, apart and together again, keeps one key
/// for the release it reads as — what a selection or a running
/// identification follows it by.
#[tokio::test]
async fn a_folders_release_keeps_its_key_across_readings() {
    let (test, scan, root) = scanned_root(&["Album/Disc 1", "Album/Disc 2"]).await;
    let manager = &test.service.library_manager;
    let grouping_of = || async {
        manager
            .load_folder_release_decisions(&root.to_string_lossy())
            .await
            .unwrap()
            .get("Album")
            .map(|reading| reading.grouping.clone())
            .expect("the scan read the album's discs as one")
    };
    let key = grouping_of().await;
    let stored_release = || async {
        manager
            .load_folder_scan_items(&root.to_string_lossy())
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
        crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
        crate::import::FolderReleaseDecision::CombineAsOneRelease,
    ] {
        ImportService::change_folder_reading(
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

/// A folder that holds the art beside its disc folders gives the art to the
/// release it reads as, and each disc a run of its own.
#[tokio::test]
async fn a_folder_read_as_one_release_takes_its_own_files_and_numbers_its_discs() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("music");
    for disc in ["Album/Disc 1", "Album/Disc 2"] {
        write_disc(&root.join(disc), 2);
    }
    std::fs::write(root.join("Album/notes.txt"), "notes").unwrap();
    let root = PathBuf::from(
        crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy()).unwrap(),
    );
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    let release = test
        .service
        .library_manager
        .load_folder_scan_items(&root.to_string_lossy())
        .await
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) => Some(candidate),
            _ => None,
        })
        .expect("the album is one release");
    assert!(release
        .files
        .release_files()
        .any(|file| file.relative_path == "notes.txt"));
    assert_eq!(
        crate::import::track_slots::direct_entry_track_rows(&release.files)
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1)), (Some(2), Some(2))]
    );
}
