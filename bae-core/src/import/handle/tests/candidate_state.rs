#[tokio::test(flavor = "multi_thread")]
async fn removing_a_watched_folder_cancels_in_flight_extraction() {
    use crate::import::extraction::{ArtworkAnalysis, ArtworkAnalyzer, ExtractionSource};
    use crate::identify::signals::TextSignal;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Counts calls and holds every one until the gate opens, so the removal
    // lands while the pass has as many reads under way as it may.
    struct HeldAnalyzer {
        calls: AtomicUsize,
        held: crate::test_gate::Held,
    }
    impl ArtworkAnalyzer for HeldAnalyzer {
        fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.held.pass();
            ArtworkAnalysis { barcodes: Vec::new(), text_lines: vec!["Line".to_string()] }
        }
    }

    // One FLAC, and more JPEGs than the OCR pass reads at once.
    fn fixture_flac() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/flac/01 Test Track 1.flac"
        ))
        .expect("read FLAC fixture")
    }
    fn minimal_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
    }

    let (manager, _, tmp) = setup_test_manager().await;
    let root = tmp.path().join("watch-root");
    std::fs::create_dir_all(&root).unwrap();
    let release_folder = root.join("Artist Name - Album Title");
    std::fs::create_dir_all(&release_folder).unwrap();
    std::fs::write(release_folder.join("01 - Track.flac"), fixture_flac()).unwrap();
    for n in 1..=crate::import::extraction::service::IMAGES_IN_FLIGHT + 2 {
        std::fs::write(release_folder.join(format!("p{n}.jpg")), minimal_jpeg()).unwrap();
    }

    let import_handle =
        manager.start_import().await.unwrap();
    let (gate, held, reached) = crate::test_gate::closed();
    let analyzer = std::sync::Arc::new(HeldAnalyzer { calls: AtomicUsize::new(0), held });
    import_handle.register_artwork_analyzer(analyzer.clone());

    let mut events = import_handle.every_event();
    import_handle.add_watched_folder(root.clone()).await.unwrap();

    // Take the key from the scanned candidate's path, however it is canonicalized.
    let candidate_path = tokio::time::timeout(HANG_GUARD, async {
        loop {
            let event = events.recv().await.expect("event channel closed");
            if let ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }) = event {
                break candidate.path;
            }
        }
    })
    .await
    .expect("timed out waiting for the folder candidate");
    let key = crate::import::CandidateKey::folder(&candidate_path);
    tokio::time::timeout(
        HANG_GUARD,
        import_handle.wait_for_list(crate::import::ImportListView::default(), |projection| {
            projection.windows.iter().any(|window| {
                window.items.iter().any(|item| match item {
                    crate::import::ImportListItem::Candidate { row, .. } => {
                        row.candidate_key == key
                    }
                    _ => false,
                })
            })
        }),
    )
    .await
    .expect("the candidate list reflects the scanned folder");
    let candidate = import_handle
        .get_release_candidate(&key)
        .await
        .expect("the candidate reads back")
        .expect("the accepted list holds the candidate");

    // Extraction alone, without a run that would ask the providers.
    let mut extraction = import_handle.extraction.start(
        import_handle.new_identification_run(),
        key.clone(),
        ExtractionSource::Candidate { candidate },
        bae_catalogs::rate_limiter::CallPriority::Interactive,
    );
    reached.recv_timeout(HANG_GUARD).expect("the OCR pass reaches its first image");
    let cancelled =
        import_handle.extraction.cancelled_for_test(&key).expect("the extraction is in flight");
    import_handle.remove_watched_folder(root.clone()).await.unwrap();
    // Let the reads under way finish only once the removal has reached the
    // extraction, so whatever the pass does next it does cancelled.
    tokio::time::timeout(HANG_GUARD, cancelled)
        .await
        .expect("removing the folder cancels the extraction");
    gate.open();
    // The extraction drops its end of the watch when it stops.
    tokio::time::timeout(HANG_GUARD, async { while extraction.changed().await.is_ok() {} })
        .await
        .expect("the cancelled extraction stops");

    while let Ok(event) = events.try_recv() {
        if let ImportEvent::SignalsUpdated { candidate_key, signals, .. } = event {
            if candidate_key == key {
                assert!(
                    !matches!(signals.text, TextSignal::Settled { .. }),
                    "extraction for a removed folder must not settle, got {:?}",
                    signals.text,
                );
            }
        }
    }
    assert!(
        analyzer.calls.load(Ordering::SeqCst) <= crate::import::extraction::service::IMAGES_IN_FLIGHT,
        "removing the folder must stop the OCR pass before the images not yet started",
    );
}

#[tokio::test]
async fn removing_a_root_queued_behind_a_decision_does_not_deadlock() {
    let (manager, database, _temp) = setup_test_manager().await;
    let tables = crate::import::tables::ImportTables::new(database);
    let root = bae_db::watched_path::host_root("/music");
    tables.add_watched_import_folder(&root).await.unwrap();
    let handle = manager.start_import().await.unwrap();
    // A row under `Collection`, so the decision the removal races is one the
    // list offers.
    let folder = root.join("Collection").join("Album");
    let generation = tables.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    crate::import::store_scan_item(&tables, &bae_diagnostics::Diagnostics::noop(),
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(
                crate::import::folder_scanner::FolderCandidate {
                    path: folder.clone(),
                    file_root: folder.clone(),
                    name: "Album".to_string(),
                    files: crate::import::folder_scanner::CategorizedFiles {
                        files: Vec::new(),
                        parts: Vec::new(),
                    },
                    watched_folder_path: root.clone(),
                    scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
                    file_edit_revision: 0,
                    relative_folder: bae_db::RelativeFolder::test("Collection/Album"),
                    grouping: None,
                },
            ),
        )
        .await
        .unwrap()
        .expect("the scan generation is current");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: root.clone(),
        relative_folder: bae_db::RelativeFolder::test("Collection"),
    };

    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    handle
        .watcher
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (key, FolderReleaseDecision::CombineAsOneRelease),
            completion: decision_completion,
        })
        .unwrap();

    let removal = handle.remove_watched_folder(root.clone());
    let (decision, removal) =
        tokio::time::timeout(HANG_GUARD, async { tokio::join!(decision_result, removal) })
            .await
            .expect("queued decision and removal deadlocked");

    // Whichever the coordinator reaches first, the decision is answered, and
    // not with success.
    assert!(decision.unwrap().is_err());
    removal.unwrap();
    tokio::task::spawn_blocking(move || handle.stop_and_join()).await.unwrap();
}
