/// 2. Folder scan produces correct candidates from a multi-album directory.
#[tokio::test]
async fn folder_scan_produces_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album1).unwrap();
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album1, &["01 Track.flac", "02 Track.flac"]);
    generate_album_files(&album2, &["01 Track.flac"]);

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();

    let mut candidates = vec![];
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), scan_rx.recv()).await {
            Ok(Some(ScanEvent::FolderCandidate { candidate: c, .. })) => {
                candidates.push(c);
            }
            Ok(Some(ScanEvent::Finished)) => break,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {
                if tokio::time::Instant::now() > deadline {
                    panic!("Scan did not finish within 5s");
                }
            }
        }
    }

    // `Collection/` has no disc-indicator subdirs, so it's a navigation
    // container. Each album inside it is its own candidate.
    assert_eq!(candidates.len(), 2, "each album should be a candidate");
    let names: std::collections::BTreeSet<_> = candidates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains("Artist - First Album"));
    assert!(names.contains("Artist - Second Album"));
}

/// The watcher reconciles a folder against the candidates it last emitted: a new
/// release folder appears as a candidate, and a deleted one emits
/// `CandidateRemoved`. Driven by `scan_watched_folders` re-triggers (plus the
/// watcher's own debounced FS reconciles), so it doesn't hinge on
/// filesystem-event timing — both paths reconcile to the same on-disk truth, so
/// each step waits for its expected candidate event regardless of how many fire.
#[tokio::test]
async fn watcher_reconciles_added_and_removed_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    fs::create_dir_all(&album1).unwrap();
    generate_album_files(&album1, &["01 Track.flac"]);
    let album1_key = album1.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();

    let first = scan_batch_until(&mut scan_rx, "the first album's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album1_key.as_str()))
    })
    .await;
    assert!(
        first.added.contains(&album1_key),
        "initial scan should surface the first album"
    );
    assert!(
        first.removed.is_empty(),
        "the initial scan of a fresh folder removes nothing"
    );

    // A new release folder appears on disk; re-scan surfaces it.
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album2, &["01 Track.flac"]);
    let album2_key = album2.to_string_lossy().into_owned();
    f.handle.scan_watched_folders().unwrap();

    let second = scan_batch_until(&mut scan_rx, "the newly-added folder's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album2_key.as_str()))
    })
    .await;
    assert!(
        second.added.contains(&album2_key),
        "the newly-added folder should surface as a candidate"
    );

    // The first release folder is deleted; re-scan removes its candidate.
    fs::remove_dir_all(&album1).unwrap();
    f.handle.scan_watched_folders().unwrap();

    let third = scan_batch_until(&mut scan_rx, "the deleted folder's candidate removal", |e| {
        matches!(e, ScanEvent::CandidateRemoved { candidate_key } if candidate_key == &album1_key)
    })
    .await;
    assert!(
        third.removed.contains(&album1_key),
        "the deleted folder's candidate should be removed"
    );
}

struct ScanBatch {
    added: Vec<String>,
    removed: Vec<String>,
}

/// Drain scan events until one matching `done` arrives, collecting the candidate
/// paths added and candidate keys removed along the way. Event-driven: returns
/// the instant the awaited event lands, and fails loud if it never does within a
/// bounded deadline. Positive assertions read the returned batch. (The watcher's
/// debounced FS reconcile can land alongside an explicit re-scan; both reconcile
/// to the same on-disk truth, so the batch is stable however many fire.)
async fn scan_batch_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut done: impl FnMut(&ScanEvent) -> bool,
) -> ScanBatch {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out after 10s waiting for {what}");
        }
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) => {
                let finished = done(&event);
                match &event {
                    ScanEvent::FolderCandidate { candidate: c, .. } => {
                        added.push(c.path.to_string_lossy().into_owned())
                    }
                    ScanEvent::CandidateRemoved { candidate_key } => {
                        removed.push(candidate_key.clone())
                    }
                    _ => {}
                }
                if finished {
                    return ScanBatch { added, removed };
                }
            }
            Ok(None) => panic!("scan channel closed before {what}"),
            Err(_) => {}
        }
    }
}

/// Wait for a single scan event matching `pred`, failing loud if none arrives
/// within a bounded deadline — the event-driven form of a fixed positive window.
async fn wait_for_scan_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut pred: impl FnMut(&ScanEvent) -> bool,
) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out after 10s waiting for {what}");
        }
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) if pred(&event) => return,
            Ok(Some(_)) => {}
            Ok(None) => panic!("scan channel closed before {what}"),
            Err(_) => {}
        }
    }
}

/// Collect every scan event that arrives within a fixed window. For the
/// negative assertions below — that after a handle call NO event of some kind
/// arrives (an unwatched folder surfaces no candidate; a redundant skip
/// re-broadcasts nothing) — where a window is the assertion, not overhead.
async fn drain_scan_events(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    window: std::time::Duration,
) -> Vec<ScanEvent> {
    let deadline = tokio::time::Instant::now() + window;
    let mut events = Vec::new();
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
            Ok(Some(event)) => events.push(event),
            Ok(None) => break,
            Err(_) => {}
        }
    }
    events
}

/// `remove_watched_folder` drops the folder from the persisted list, drops its
/// candidates from the reducer, and broadcasts the shortened list (plus sending
/// the watcher an `Unwatch`). Exercises the handle's remove path and
/// `watched_folders` accessor.
#[tokio::test]
async fn remove_watched_folder_drops_folder_and_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();
    let collection_key = collection.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection_key.clone())
        .await
        .unwrap();

    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(
        batch.added.contains(&album_key),
        "initial scan should surface the album candidate"
    );
    assert_eq!(
        f.handle
            .watched_folders()
            .iter()
            .map(|w| w.path.clone())
            .collect::<Vec<_>>(),
        vec![collection_key.clone()],
    );
    assert!(
        !f.handle
            .get_import_candidates()
            .folder_candidates
            .is_empty(),
        "the reducer holds the scanned candidate"
    );

    f.handle
        .remove_watched_folder(collection_key.clone())
        .await
        .unwrap();

    // The list accessor and the reducer both reflect the removal synchronously.
    assert!(
        f.handle.watched_folders().is_empty(),
        "removed folder is gone from the persisted list"
    );
    assert!(
        f.handle
            .get_import_candidates()
            .folder_candidates
            .is_empty(),
        "the reducer dropped the removed folder's candidates"
    );

    // The shortened (now empty) list is broadcast.
    wait_for_scan_event(
        &mut scan_rx,
        "the shortened folder-list broadcast",
        |event| matches!(event, ScanEvent::WatchedFoldersChanged { folders } if folders.is_empty()),
    )
    .await;

    // The watcher actually stopped: a new release folder appearing under the
    // now-unwatched root produces no scan activity (the reconcile that would
    // surface it never runs).
    let new_album = collection.join("Artist - Second Album");
    fs::create_dir_all(&new_album).unwrap();
    generate_album_files(&new_album, &["01 Track.flac"]);
    let after_unwatch = drain_scan_events(&mut scan_rx, std::time::Duration::from_secs(2)).await;
    assert!(
        !after_unwatch
            .iter()
            .any(|event| matches!(event, ScanEvent::FolderCandidate { candidate: _, .. })),
        "an unwatched folder must not surface new candidates, got {after_unwatch:?}",
    );
}

#[tokio::test]
async fn unavailable_watched_folder_remains_durable_and_reports_scan_failure() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let missing = f.temp_path().join("does-not-exist");
    let missing_key = missing.to_string_lossy().into_owned();

    f.handle
        .add_watched_folder(missing_key.clone())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = f.handle.get_import_candidates();
        if snapshot.folder_scan_statuses.iter().any(|status| {
            status.watched_folder_path == missing_key
                && matches!(
                    status.status,
                    bae_core::import::FolderScanStatus::Failed { .. }
                )
        }) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "unavailable watched root did not report a failed scan"
        );
        tokio::task::yield_now().await;
    }
    assert_eq!(f.handle.watched_folders().len(), 1);
}

/// A refresh is the awaited scan operation. If a watched root disappears, it
/// reports the failure, records the failed status, and preserves the last
/// candidate snapshot rather than turning an unavailable filesystem into
/// removals.
#[tokio::test]
async fn refresh_missing_watched_folder_fails_and_preserves_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("unplugged-drive");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();
    let album_key = album.to_string_lossy().into_owned();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    f.handle
        .refresh_watched_folder(root_key.clone())
        .await
        .unwrap();

    fs::remove_dir_all(&root).unwrap();
    let result = f.handle.refresh_watched_folder(root_key.clone()).await;
    assert!(
        result.is_err(),
        "refreshing a missing watched folder must fail"
    );
    let snapshot = f.handle.get_import_candidates();
    assert!(snapshot.folder_candidates.iter().any(|candidate| candidate
        .candidate
        .path
        .to_string_lossy()
        == album_key));
    assert!(snapshot.folder_scan_statuses.iter().any(|status| {
        status.watched_folder_path == root_key
            && matches!(
                status.status,
                bae_core::import::FolderScanStatus::Failed { .. }
            )
    }));
}

/// `set_candidate_skipped` flips the reducer's skip flag and broadcasts
/// `CandidateSkipChanged`; a no-op request (already in the target state) changes
/// nothing and emits nothing.
#[tokio::test]
async fn set_candidate_skipped_flips_flag_and_is_idempotent() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(batch.added.contains(&album_key));

    let skipped_of = |f: &ImportFixture| {
        f.handle
            .get_import_candidates()
            .folder_candidates
            .iter()
            .find(|c| c.candidate.path == album)
            .expect("candidate present")
            .skipped
    };
    assert!(!skipped_of(&f), "candidate starts unskipped");

    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    assert!(skipped_of(&f), "skip flag flips in the reducer");
    wait_for_scan_event(
        &mut scan_rx,
        "the CandidateSkipChanged broadcast",
        |event| {
            matches!(
                event,
                ScanEvent::CandidateSkipChanged { candidate_key, skipped }
                    if candidate_key == &album_key && *skipped
            )
        },
    )
    .await;

    // A redundant skip=true request is a no-op: no event, flag unchanged.
    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    assert!(skipped_of(&f));
    let events = drain_scan_events(&mut scan_rx, std::time::Duration::from_millis(300)).await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ScanEvent::CandidateSkipChanged { .. })),
        "a redundant skip must not re-broadcast, got {events:?}",
    );

    f.handle
        .set_candidate_skipped(album_key.clone(), false)
        .await
        .unwrap();
    assert!(!skipped_of(&f), "unskip flips the flag back");
}
