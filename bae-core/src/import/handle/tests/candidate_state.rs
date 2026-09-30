#[tokio::test(flavor = "multi_thread")]
async fn removing_a_watched_folder_cancels_in_flight_extraction() {
    use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer, ExtractionSource, TextSignal};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // Counts calls and holds the first until the gate opens, so the removal
    // lands mid-pass.
    struct HeldAnalyzer {
        calls: AtomicUsize,
        held: crate::test_gate::Held,
    }
    impl ArtworkAnalyzer for HeldAnalyzer {
        fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.held.pass();
            }
            ArtworkAnalysis {
                barcodes: Vec::new(),
                text_lines: vec!["Line".to_string()],
            }
        }
    }

    // One FLAC and three JPEGs for the OCR pass.
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

    let (manager, tmp) = setup_test_manager().await;
    let root = tmp.path().join("watch-root");
    std::fs::create_dir_all(&root).unwrap();
    let release_folder = root.join("Artist Name - Album Title");
    std::fs::create_dir_all(&release_folder).unwrap();
    std::fs::write(release_folder.join("01 - Track.flac"), fixture_flac()).unwrap();
    for img in ["p1.jpg", "p2.jpg", "p3.jpg"] {
        std::fs::write(release_folder.join(img), minimal_jpeg()).unwrap();
    }

    let import_handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let (gate, held, reached) = crate::test_gate::closed();
    let analyzer = std::sync::Arc::new(HeldAnalyzer {
        calls: AtomicUsize::new(0),
        held,
    });
    import_handle.register_artwork_analyzer(analyzer.clone());

    let mut events = import_handle.every_event();
    import_handle
        .add_watched_folder(root.to_string_lossy().to_string())
        .await
        .unwrap();

    // Take the key from the scanned candidate's path, however it is canonicalized.
    let candidate_path = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("event channel closed");
            if let ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }) = event {
                break candidate.path;
            }
        }
    })
    .await
    .expect("timed out waiting for the folder candidate");
    let key = candidate_path.to_string_lossy().to_string();
    tokio::time::timeout(
        Duration::from_secs(10),
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
        crate::util::rate_limiter::CallPriority::Interactive,
    );
    reached
        .recv_timeout(Duration::from_secs(10))
        .expect("the OCR pass reaches its first image");
    import_handle
        .remove_watched_folder(root.to_string_lossy().to_string())
        .await
        .unwrap();
    gate.open();
    // The extraction drops its end of the watch when it stops.
    tokio::time::timeout(Duration::from_secs(10), async {
        while extraction.changed().await.is_ok() {}
    })
    .await
    .expect("the cancelled extraction stops");

    while let Ok(event) = events.try_recv() {
        if let ImportEvent::SignalsUpdated {
            candidate_key,
            signals,
            ..
        } = event
        {
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
        analyzer.calls.load(Ordering::SeqCst) < 3,
        "removing the folder must stop the OCR pass early",
    );
}

#[tokio::test]
async fn removing_a_root_queued_behind_a_decision_does_not_deadlock() {
    let (manager, _temp) = setup_test_manager().await;
    let root = PathBuf::from(crate::import::watched_folder::host_root("/music"));
    manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    // A row under `Collection`, so the decision the removal races is one the
    // list offers.
    let folder = root.join("Collection").join("Album");
    let generation = manager
        .begin_folder_scan(&root.to_string_lossy())
        .await
        .unwrap();
    manager
        .save_folder_scan_item(
            &root.to_string_lossy(),
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(
                crate::import::folder_scanner::FolderCandidate {
                    path: folder.clone(),
                    file_root: folder.clone(),
                    name: "Album".to_string(),
                    files: crate::import::folder_scanner::CategorizedFiles {
                        files: Vec::new(), parts: Vec::new(), 
                    },
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
                    file_edit_revision: 0,
                    display_path: "Collection/Album".to_string(),
                    grouping: None,
                },
            ),
        )
        .await
        .unwrap()
        .expect("the scan generation is current");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: root.to_string_lossy().into_owned(),
        relative_folder_path: "Collection".to_string(),
    };

    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    handle
        .watcher
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (key, FolderReleaseDecision::CombineAsOneRelease),
            completion: decision_completion,
        })
        .unwrap();

    let removal = handle.remove_watched_folder(root.to_string_lossy().into_owned());
    let (decision, removal) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(decision_result, removal)
    })
    .await
    .expect("queued decision and removal deadlocked");

    // Whichever the coordinator reaches first, the decision is answered, and
    // not with success.
    assert!(decision.unwrap().is_err());
    removal.unwrap();
    tokio::task::spawn_blocking(move || handle.stop_and_join())
        .await
        .unwrap();
}
