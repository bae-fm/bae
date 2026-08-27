#[tokio::test(flavor = "multi_thread")]
async fn removing_a_watched_folder_cancels_in_flight_extraction() {
    use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer, ExtractionSource, TextSignal};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // Local delayed OCR stub: counts `analyze` calls so the test can assert the
    // pass stopped early, and sleeps each call so a cancel lands mid-pass.
    struct DelayedAnalyzer {
        calls: AtomicUsize,
        delay: Duration,
    }
    impl ArtworkAnalyzer for DelayedAnalyzer {
        fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            ArtworkAnalysis {
                barcodes: Vec::new(),
                text_lines: vec!["Line".to_string()],
            }
        }
    }

    // Minimal on-disk release: one MP3 (satisfies the audio gate) and three
    // JPEGs for the OCR pass to iterate.
    fn minimal_mp3() -> Vec<u8> {
        let mut v = Vec::with_capacity(32);
        v.extend_from_slice(b"ID3");
        v.resize(32, 0);
        v
    }
    fn minimal_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
    }

    let (manager, tmp) = setup_test_manager().await;
    let root = tmp.path().join("watch-root");
    std::fs::create_dir_all(&root).unwrap();
    let release_folder = root.join("Artist Name - Album Title");
    std::fs::create_dir_all(&release_folder).unwrap();
    std::fs::write(release_folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
    for img in ["p1.jpg", "p2.jpg", "p3.jpg"] {
        std::fs::write(release_folder.join(img), minimal_jpeg()).unwrap();
    }

    let import_handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let (_identify, extraction) = import_handle.start_candidate_services();
    let analyzer = std::sync::Arc::new(DelayedAnalyzer {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(200),
    });
    extraction.register_analyzer(analyzer.clone());

    let mut events = import_handle.subscribe_events();
    import_handle
        .add_watched_folder(root.to_string_lossy().to_string())
        .await
        .unwrap();

    // Wait for the scan to surface the release as a candidate. Take the key
    // from the emitted candidate's path (robust to path canonicalization).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let candidate_path = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .expect("timed out waiting for the folder candidate")
            .expect("event channel closed");
        if let ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }) = event {
            break candidate.path;
        }
    };
    let key = candidate_path.to_string_lossy().to_string();
    tokio::time::timeout(
        Duration::from_secs(5),
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
    let files = import_handle
        .get_candidate(&key)
        .await
        .expect("the candidate reads back")
        .and_then(|snapshot| match snapshot {
            crate::import::ImportCandidateSnapshot::Folder { candidate, .. } => {
                Some(candidate.files)
            }
            _ => None,
        })
        .expect("the accepted list holds the candidate");

    // Start extraction the way the bridge does, then remove the folder mid-OCR.
    extraction.start(
        key.clone(),
        ExtractionSource::Folder {
            path: candidate_path.clone(),
            files,
        },
        crate::util::rate_limiter::CallPriority::Interactive,
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    import_handle
        .remove_watched_folder(root.to_string_lossy().to_string())
        .await
        .unwrap();

    // Drain this key's SignalsUpdated until quiet: the cancelled run never settles.
    loop {
        match tokio::time::timeout(Duration::from_millis(500), events.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
                ..
            })) if candidate_key == key => {
                assert!(
                    !matches!(signals.text, TextSignal::Settled { .. }),
                    "extraction for a removed folder must not settle, got {:?}",
                    signals.text,
                );
            }
            Ok(Ok(_)) => continue,
            _ => break,
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
    let root = PathBuf::from(crate::import::folder_registry::host_root("/music"));
    manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    handle
        .folder_registry
        .lock()
        .unwrap()
        .apply_added(root.to_string_lossy().into_owned());
    // A row under `Collection`, so the decision the removal races is one the
    // list really offers: a header over rows can be read as one release.
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
                        files: Vec::new(),
                        format_label: "FLAC".to_string(),
                    },
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
                    file_edit_revision: 0,
                    display_path: "Collection/Album".to_string(),
                    resolved_boundaries: Vec::new(),
                    combine_ancestor_key: None,
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
        .watcher_tx
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (key, FolderReleaseDecision::CombineAsOneRelease),
            completion: decision_completion,
        })
        .unwrap();

    let removal = handle.remove_watched_folder(root.to_string_lossy().into_owned());
    let (decision, removal) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(decision_result, removal)
    })
    .await
    .expect("queued decision and removal deadlocked");

    assert_eq!(
        decision.unwrap(),
        Err(format!("{} is no longer watched", root.display()))
    );
    removal.unwrap();
    tokio::task::spawn_blocking(move || handle.stop_and_join())
        .await
        .unwrap();
}
