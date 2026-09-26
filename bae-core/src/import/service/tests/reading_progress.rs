#[tokio::test]
async fn reading_progress_advances_while_coven_prepares_a_dominant_file() {
    let mut test = setup_import_service().await;
    test.service.event_tx =
        crate::import::ImportEventBus::new(1024, crate::import::CandidateRuntime::default());
    // The import under test commits a draft it was handed, not one the folder's
    // tags wrote: the pre-fill would give the candidate a file-metadata draft whose
    // stored reading this import is not carrying.
    test.service
        .library_manager
        .set_prefill_with_file_metadata(false).await
        .unwrap();
    let folder = test.temp.path().join("reading-progress-candidate");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("01-payload.bin"), vec![0x5a; 1024 * 1024]).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("00-track.flac"),
    )
    .unwrap();
    let (candidate_key, candidate) =
        store_scanned_candidate(&test, &folder, "Reading Progress Candidate").await;
    let service = &test.service;

    let mut events = service.event_tx.subscribe();
    service
        .prepare_and_run_folder_import(
            "import-reading-progress".to_string(),
            candidate_key.clone(),
            crate::import::release_candidate::CandidateSource {
                path: folder,
                scope: crate::import::ReleaseFileScope::Recursive, parts: Vec::new(), 
            },
            super::ImportExpectation {
                candidate,
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
        reading_percents.windows(2).all(|pair| pair[0] <= pair[1]),
        "the candidate's Reading Files progress must never move backward: {reading_percents:?}",
    );
}
