#[tokio::test]
async fn reading_progress_advances_while_coven_prepares_a_dominant_file() {
    let (mut service, tmp) = setup_import_service().await;
    let (event_tx, _) = tokio::sync::broadcast::channel(1024);
    service.event_tx = event_tx;
    let folder = tmp.path().join("reading-progress-candidate");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("01-payload.bin"), vec![0x5a; 1024 * 1024]).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("00-track.flac"),
    )
    .unwrap();

    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let expected_content_hash = files.content_hash();
    let candidate_key = folder.to_string_lossy().into_owned();
    service
        .library_manager
        .add_watched_import_folder(&candidate_key)
        .await
        .unwrap();
    let generation = service
        .library_manager
        .begin_folder_scan(&candidate_key)
        .await
        .unwrap();
    service
        .library_manager
        .save_folder_scan_item(
            &candidate_key,
            generation,
            &ScanItem::Valid(crate::import::folder_scanner::FolderCandidate {
                path: folder.clone(),
                file_root: folder.clone(),
                name: "Reading Progress Candidate".to_string(),
                files,
                watched_folder_path: candidate_key.clone(),
                scope: crate::import::ReleaseFileScope::Recursive,
                file_edit_revision: 0,
                display_path: "Reading Progress Candidate".to_string(),
                resolved_boundaries: Vec::new(),
                combine_ancestor_key: None,
            }),
        )
        .await
        .unwrap()
        .expect("the stored scan generation is current");
    service
        .library_manager
        .finish_folder_scan(&candidate_key, generation, None)
        .await
        .unwrap();
    let metadata_revision = prepare_named_candidate(
        &service,
        &expected_content_hash,
        &candidate_key,
        &folder.to_string_lossy(),
        "Reading Progress Candidate",
    )
    .await;

    let mut events = service.event_tx.subscribe();
    service
        .prepare_and_run_folder_import(
            "import-reading-progress".to_string(),
            candidate_key.clone(),
            crate::import::release_candidate::CandidateSource::Folder {
                path: folder,
                scope: crate::import::ReleaseFileScope::Recursive,
            },
            super::ImportExpectation {
                content_hash: expected_content_hash,
                edit_revision: 0,
                metadata_revision,
                file_tag_snapshot: None,
            },
            StorageMode::Local,
            false,
        )
        .await
        .unwrap();

    let mut reading_percents = Vec::new();
    loop {
        match events.try_recv() {
            Ok(crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    ImportProgress::Progress {
                        percent: Some(percent),
                        phase: ImportPhase::ReadingFiles,
                        ..
                    },
                ..
            }) => reading_percents.push(percent),
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(error) => panic!("import progress event stream failed: {error}"),
        }
    }
    assert_eq!(reading_percents.first(), Some(&0));
    assert_eq!(reading_percents.last(), Some(&100));
    assert!(
        reading_percents
            .iter()
            .any(|percent| (1..25).contains(percent)),
        "a release dominated by one file must advance before that file finishes: {reading_percents:?}",
    );
    assert!(
        reading_percents
            .windows(2)
            .all(|pair| pair[0] <= pair[1]),
        "the candidate's Reading Files progress must never move backward: {reading_percents:?}",
    );
}
