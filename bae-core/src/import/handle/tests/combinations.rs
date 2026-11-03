use super::*;

/// Pending rows the queue holds, by display path, in path order.
async fn pending(handle: &ImportServiceHandle) -> Vec<String> {
    let projection = handle
        .load_list(
            crate::import::ImportListView {
                order: crate::import::ImportListOrder::PathAscending,
                ..crate::import::ImportListView::default()
            },
            [bae_db::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
        )
        .await
        .unwrap();
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.display_path.clone()),
            _ => None,
        })
        .collect()
}

/// Folders picked together leave the queue as one release, and separating it
/// brings them back as they were, drafts and all.
#[tokio::test(flavor = "multi_thread")]
async fn separating_picked_folders_returns_them_as_they_were() {
    let (parts, database, _library) = setup_test_import().await;
    let tables = crate::import::tables::ImportTables::new(database);
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
    let order: Vec<crate::import::CandidateKey> = members.into_iter().map(|(key, _)| key).collect();
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::import::tables::GroupingFacts::Picked { members }) if members == order
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
/// same reading the header's "Combine as One Release" writes. Separating it
/// keeps the folder's releases apart.
#[tokio::test(flavor = "multi_thread")]
async fn combining_every_release_under_a_folder_reads_that_folder_as_one() {
    let (parts, database, _library) = setup_test_import().await;
    let tables = crate::import::tables::ImportTables::new(database);
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("music");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for live in ["Live A", "Live B"] {
        let folder = root.join("Artist").join(live);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let root = bae_db::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = parts.start().await.unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    assert_eq!(
        pending(&handle).await,
        vec!["Artist/Live A".to_string(), "Artist/Live B".to_string()]
    );
    let keys: Vec<crate::import::CandidateKey> = ["Live A", "Live B"]
        .iter()
        .map(|live| crate::import::CandidateKey::folder(&root.join("Artist").join(live)))
        .collect();

    let key = handle.combine_candidates(keys.clone()).await.unwrap();
    assert_eq!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::import::tables::GroupingFacts::Anchored {
            folder: crate::import::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder: bae_db::RelativeFolder::test("Artist"),
            },
            decision: crate::import::FolderReleaseDecision::CombineAsOneRelease,
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
        Some(crate::import::tables::GroupingFacts::Anchored {
            decision: crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
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
    let (parts, database, _library) = setup_test_import().await;
    let tables = crate::import::tables::ImportTables::new(database);
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("music");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for live in ["Live 2", "Live 3", "Live 10"] {
        let folder = root.join("Artist").join(live);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let root = bae_db::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = parts.start().await.unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    let key_of = |live: &str| crate::import::CandidateKey::folder(&root.join("Artist").join(live));

    // Live 3 stays out, so the pick is not every release under `Artist/`.
    let key = handle.combine_candidates(vec![key_of("Live 10"), key_of("Live 2")]).await.unwrap();
    assert!(matches!(
        tables.load_grouping(&key).await.unwrap(),
        Some(crate::import::tables::GroupingFacts::Picked { .. })
    ));

    assert_eq!(
        handle.separate_candidate(&key).await.unwrap(),
        vec![key_of("Live 2"), key_of("Live 10")]
    );
    shut_down(handle).await;
}
