/// Each Pending group header with whether it can combine, and each row with
/// whether it can separate.
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
            live_standings: Default::default(),
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

/// One album read from two disc folders offers to separate, and its artist's
/// header has nothing to combine.
#[tokio::test]
async fn one_grouped_album_under_an_artist_offers_no_combine() {
    let (test, _scan, _root) =
        scanned_root(&["Artist/Album/Disc 1", "Artist/Album/Disc 2"]).await;
    let (headers, rows) = offers(&test.service.library_manager).await;
    assert_eq!(headers, vec![("Artist".to_string(), false)]);
    assert_eq!(rows, vec![("Artist/Album".to_string(), true)]);
}

/// Two albums under an artist's folder offer to combine from the header, and
/// neither separates.
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

/// Disc folders read as one album separate from its row, and once kept apart
/// combine from the header over them.
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

/// A folder read apart and together again keeps its release's key.
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

/// A folder's own files go to the release it reads as, and each disc is
/// numbered on its own.
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
        crate::import::audio_layout::direct_entry_track_rows(&release.files)
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1)), (Some(2), Some(2))]
    );
}
