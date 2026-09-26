use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn selected_folders_from_different_roots_import_as_one_release() {
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (first, first_key, _) = picked_candidate(&manager, &first_root, "Volume B").await;
    let (second, second_key, _) = picked_candidate(&manager, &second_root, "Volume A").await;
    let spare_root = TempDir::new().unwrap();
    let (_spare, spare_key, _) = picked_candidate(&manager, &spare_root, "Volume C").await;
    let original_paths = first
        .files
        .release_files()
        .chain(second.files.release_files())
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    // Combining takes the selection, not an order: core plays the folders in
    // key order and names the release after the first of them.
    let mut ordered = vec![first_key.clone(), second_key.clone()];
    ordered.sort();
    let leading = if ordered[0] == first_key {
        &first
    } else {
        &second
    };
    let key = handle
        .combine_candidates(vec![second_key.clone(), first_key.clone()])
        .await
        .unwrap();
    assert_eq!(
        handle.candidate_source_folders(&key).await.unwrap(),
        ordered
    );
    assert!(handle
        .get_release_candidate(&key)
        .await
        .unwrap()
        .is_some_and(|candidate| candidate.grouping.as_deref() == Some(key.as_str())));
    assert!(handle
        .get_release_candidate(&first_key)
        .await
        .unwrap()
        .is_none());
    assert!(handle
        .get_release_candidate(&second_key)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        pane(&handle, &key).await.metadata_draft.album_title,
        leading.name
    );
    // A combined key is not a folder to combine again; separating it first is.
    // Every folder key sorts ahead of the combination key, so the folder in
    // this selection is the one loaded first and the combination is what the
    // call stops on.
    assert!(handle
        .combine_candidates(vec![key.clone(), spare_key.clone()])
        .await
        .unwrap_err()
        .to_string()
        .contains("separate an existing combination before combining its folders again"));
    handle
        .set_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::import::DraftFieldEdit::Text {
                field: crate::import::CandidateEditField::AlbumTitle,
                value: "Collected Volumes".into(),
            },
        )
        .await
        .unwrap();
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::import::ArtistAssignment::named("Combined Artist")],
        )
        .await
        .unwrap();
    let projected = pane(&handle, &key).await;
    assert_eq!(projected.metadata_draft.tracks.len(), 4);
    assert_eq!(
        projected
            .metadata_draft
            .tracks
            .iter()
            .map(|track| track.side)
            .collect::<Vec<_>>(),
        [Some(1), Some(1), Some(2), Some(2)]
    );
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let release_id = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match events.recv().await.unwrap() {
                ImportEvent::ImportProgress {
                    progress:
                        ImportProgress::Complete {
                            import_id: completed,
                            id,
                            ..
                        },
                    ..
                } if completed == import_id => break id,
                ImportEvent::ImportProgress {
                    progress:
                        ImportProgress::Failed {
                            import_id: failed,
                            error,
                        },
                    ..
                } if failed == import_id => panic!("combined import failed: {error}"),
                _ => {}
            }
        }
    })
    .await
    .expect("combined import reports a terminal result");
    let imported = handle
        .library_manager
        .release_edit_seed(&release_id)
        .await
        .unwrap();
    assert_eq!(imported.edit.album_title, "Collected Volumes");
    assert_eq!(imported.edit.tracks.len(), 4);
    assert_eq!(
        imported
            .edit
            .tracks
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        [
            (Some(1), Some(1)),
            (Some(1), Some(2)),
            (Some(2), Some(1)),
            (Some(2), Some(2))
        ]
    );
    assert!(pane(&handle, &key).await.is_added);
    assert!(handle.separate_candidate(&key).await.is_err());
    assert!(original_paths.iter().all(|path| path.is_file()));
    shut_down(handle).await;
}

/// Pending rows the queue holds, by display path, in path order.
async fn pending(handle: &ImportServiceHandle) -> Vec<String> {
    let projection = handle
        .library_manager
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
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (first, first_key, _) = picked_candidate(&manager, &first_root, "Volume A").await;
    let (_, second_key, _) = picked_candidate(&manager, &second_root, "Volume B").await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let before = pane(&handle, &first_key).await.metadata_draft;
    assert_eq!(pending(&handle).await.len(), 2);

    let key = handle
        .combine_candidates(vec![first_key.clone(), second_key.clone()])
        .await
        .unwrap();
    // The two roots are temporary folders, so which one sorts first — and so
    // plays first and names the release — is the order of their keys.
    let mut members = vec![(first_key.clone(), "Volume A"), (second_key.clone(), "Volume B")];
    members.sort();
    assert_eq!(pending(&handle).await, vec![members[0].1.to_string()]);
    let order: Vec<String> = members.into_iter().map(|(key, _)| key).collect();
    assert!(matches!(
        handle.library_manager.load_grouping(&key).await.unwrap(),
        Some(crate::db::GroupingFacts::Picked { members }) if members == order
    ));

    handle.separate_candidate(&key).await.unwrap();
    assert!(handle.library_manager.load_grouping(&key).await.unwrap().is_none());
    assert!(handle.get_release_candidate(&key).await.unwrap().is_none());
    assert_eq!(pending(&handle).await.len(), 2);
    assert_eq!(pane(&handle, &first_key).await.metadata_draft, before);
    assert_eq!(
        handle
            .get_release_candidate(&first_key)
            .await
            .unwrap()
            .unwrap()
            .files,
        first.files
    );
    shut_down(handle).await;
}

/// Picking every release below one folder reads that folder as one release,
/// the way the scan reads a folder of disc parts: the same action, and the
/// same reading the header's "Combine as One Release" writes. Separating it
/// keeps the folder's releases apart.
#[tokio::test(flavor = "multi_thread")]
async fn combining_every_release_under_a_folder_reads_that_folder_as_one() {
    let (manager, _library) = setup_test_manager().await;
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("music");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for live in ["Live A", "Live B"] {
        let folder = root.join("Artist").join(live);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&fixture, folder.join("01.flac")).unwrap();
    }
    let root = crate::import::watched_folder::canonical_absolute_root(&root.to_string_lossy())
        .unwrap();
    manager.add_watched_import_folder(&root).await.unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    handle.refresh_watched_folder(root.clone()).await.unwrap();
    assert_eq!(
        pending(&handle).await,
        vec!["Artist/Live A".to_string(), "Artist/Live B".to_string()]
    );
    let keys: Vec<String> = ["Live A", "Live B"]
        .iter()
        .map(|live| {
            PathBuf::from(&root)
                .join("Artist")
                .join(live)
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    let key = handle.combine_candidates(keys).await.unwrap();
    assert_eq!(
        handle.library_manager.load_grouping(&key).await.unwrap(),
        Some(crate::db::GroupingFacts::Anchored {
            folder: crate::import::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder_path: "Artist".into(),
            },
            decision: crate::import::FolderReleaseDecision::CombineAsOneRelease,
        })
    );
    assert_eq!(pending(&handle).await, vec!["Artist".to_string()]);
    let release = handle.get_release_candidate(&key).await.unwrap().unwrap();
    assert_eq!(release.files.parts.len(), 2);

    handle.separate_candidate(&key).await.unwrap();
    assert_eq!(
        pending(&handle).await,
        vec!["Artist/Live A".to_string(), "Artist/Live B".to_string()]
    );
    assert!(matches!(
        handle.library_manager.load_grouping(&key).await.unwrap(),
        Some(crate::db::GroupingFacts::Anchored {
            decision: crate::import::FolderReleaseDecision::KeepAsSeparateReleases,
            ..
        })
    ));
    shut_down(handle).await;
}
