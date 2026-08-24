#[tokio::test]
async fn coordinator_coalesces_same_root_to_one_followup_scan() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.commands.send(WatcherCommand::Rescan(root)).unwrap();
    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    tokio::task::yield_now().await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 2);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_waits_for_the_active_scan_to_finish() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;

    let mut result = Box::pin(result);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), result.as_mut())
            .await
            .is_err(),
        "removal completed while the scan could still install a late watch"
    );

    harness.scans.complete(0);
    assert_eq!(result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_coalesces_duplicate_removals_for_one_root() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (first_completion, first_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root.clone(),
            completion: first_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    let (second_completion, second_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion: second_completion,
        })
        .unwrap();

    harness.scans.complete(0);
    assert_eq!(first_result.await.unwrap(), Ok(()));
    assert_eq!(second_result.await.unwrap(), Ok(()));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "remove"]
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_blocked_root_removal_does_not_block_another_roots_refresh() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music/one")))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (remove_completion, remove_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root_path("/music/one"),
            completion: remove_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;

    let (refresh_completion, refresh_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music/two"),
            completion: refresh_completion,
        })
        .unwrap();
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), refresh_result)
            .await
            .expect("another root's refresh was blocked by removal")
            .unwrap(),
        Ok(())
    );

    assert!(
        tokio::time::timeout(Duration::from_millis(50), remove_result)
            .await
            .is_err(),
        "removal completed before its blocked scan"
    );
    harness.scans.complete(0);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_join_failure_restores_a_runnable_root_schedule() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.abort(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("folder scan task failed while removing"));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_uninstall_failure_restores_a_runnable_root_schedule() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.uninstall_error.lock().unwrap() =
        Some("injected uninstall failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected uninstall failure"));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall"]
    );
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_database_failure_reinstalls_and_rescans_before_returning() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected database failure"));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "remove", "reinstall"]
    );
    assert_eq!(
        harness.folder_registry.lock().unwrap().watched_folders(),
        vec![crate::import::WatchedFolder::from_path(host_root("/music"))]
    );
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_blocked_reinstall_does_not_block_another_roots_persistence() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    harness
        .removal_backend
        .block_reinstall
        .store(true, std::sync::atomic::Ordering::SeqCst);
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music/one")))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (remove_completion, remove_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root_path("/music/one"),
            completion: remove_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0);
    tokio::time::timeout(
        Duration::from_secs(2),
        harness.removal_backend.reinstall_started.notified(),
    )
    .await
    .expect("failed durable removal did not start watch restoration");

    let (refresh_completion, refresh_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music/two"),
            completion: refresh_completion,
        })
        .unwrap();
    harness.scans.wait_for_count(2).await;
    let other_root_commit = tokio::time::timeout(
        Duration::from_millis(50),
        harness.folder_state_commit.lock(),
    )
    .await;
    let other_root_was_blocked = other_root_commit.is_err();
    drop(other_root_commit);

    harness.removal_backend.release_reinstall.notify_one();
    harness.scans.complete(1);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), refresh_result)
            .await
            .expect("another root's refresh did not complete")
            .unwrap(),
        Ok(())
    );
    assert!(
        remove_result.await.unwrap().is_err(),
        "injected durable removal failure was not returned"
    );
    harness.scans.wait_for_count(3).await;
    harness.scans.complete(2);
    harness.shutdown().await;

    assert!(
        !other_root_was_blocked,
        "watch restoration held the persistence guard needed by another root"
    );
}

#[tokio::test]
async fn coordinator_removal_database_and_restore_failures_return_both_errors() {
    let harness = CoordinatorHarness::new().await;
    *harness.removal_backend.remove_error.lock().unwrap() =
        Some("injected database failure".to_string());
    *harness.removal_backend.reinstall_error.lock().unwrap() =
        Some("injected restore failure".to_string());
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness.scans.complete(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected database failure"));
    assert!(error.contains("injected restore failure"));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_runs_different_roots_concurrently() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    for root in ["/music/one", "/music/two"] {
        harness
            .commands
            .send(WatcherCommand::Rescan(root_path(root)))
            .unwrap();
    }
    harness.scans.wait_for_count(2).await;
    assert!(!harness.scans.cancellation(0).is_cancelled());
    assert!(!harness.scans.cancellation(1).is_cancelled());
    harness.scans.complete(0);
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A refresh waits for its scan to be over, not for it to have worked. What a
/// scan made of the folder is the root's stored status and the failure event
/// the desktops raise as an alert; a refresh caller that reported it a second
/// time would put two dialogs on screen for one broken folder.
#[tokio::test]
async fn coordinator_completes_refresh_waiter_once_its_scan_is_over() {
    let harness = CoordinatorHarness::new().await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music"),
            completion,
        })
        .unwrap();
    harness.scans.wait_for_count(1).await;
    harness.scans.complete(0);
    assert_eq!(result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// A read is not a change. Linux's inotify backend reports every `open()`
/// under a watched root, so the scan's own reads — the directory walk, the rip
/// log, the CUE, the audio probe — arrive here as events about the folder the
/// scan just finished. Scanning on them would make every scan schedule the
/// next one for as long as the folder stays watched, and each of those scans
/// republishes its candidates as tentative on the way to valid, so a queue
/// sweep reading the list mid-rescan finds nothing to answer.
#[tokio::test]
async fn a_file_opened_under_a_watched_root_starts_no_scan() {
    use notify::event::{AccessKind, AccessMode};

    let harness = CoordinatorHarness::with_roots(&["/music", "/downloads"]).await;
    harness
        .fs_events
        .send(Ok(vec![
            debounced_event(
                notify::EventKind::Access(AccessKind::Open(AccessMode::Any)),
                root_path("/music").join("Album/01.flac"),
            ),
            debounced_event(
                notify::EventKind::Access(AccessKind::Close(AccessMode::Read)),
                root_path("/music").join("Album/01.flac"),
            ),
        ]))
        .unwrap();
    // A second batch the coordinator handles after the first, naming a root the
    // reads did not touch: when its scan starts, the read batch is answered.
    harness
        .fs_events
        .send(Ok(vec![debounced_event(
            notify::EventKind::Create(notify::event::CreateKind::File),
            root_path("/downloads").join("Album/01.flac"),
        )]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(
        harness.scans.path(0),
        root_path("/downloads"),
        "the only scan is the created file's; the opened file started none"
    );

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// The other half of the rule above: the close that ended a write is how a
/// finished copy announces itself on Linux, so it re-scans the root it landed
/// under.
#[tokio::test]
async fn a_finished_write_under_a_watched_root_starts_a_scan() {
    use notify::event::{AccessKind, AccessMode};

    let harness = CoordinatorHarness::new().await;
    harness
        .fs_events
        .send(Ok(vec![debounced_event(
            notify::EventKind::Access(AccessKind::Close(AccessMode::Write)),
            root_path("/music").join("Album/01.flac"),
        )]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root_path("/music"));

    harness.scans.complete(0);
    harness.shutdown().await;
}

fn debounced_event(
    kind: notify::EventKind,
    path: PathBuf,
) -> notify_debouncer_full::DebouncedEvent {
    notify_debouncer_full::DebouncedEvent::new(
        notify::Event::new(kind).add_path(path),
        std::time::Instant::now(),
    )
}

#[tokio::test]
async fn coordinator_completes_scan_while_filesystem_batches_remain_ready() {
    let harness = CoordinatorHarness::new().await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path("/music"),
            completion,
        })
        .unwrap();
    harness.scans.wait_for_count(1).await;
    for _ in 0..10_000 {
        harness.fs_events.send(Ok(Vec::new())).unwrap();
    }
    harness.scans.complete(0);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), result)
            .await
            .expect("ready filesystem batches starved scan completion")
            .unwrap(),
        Ok(())
    );
    harness.shutdown().await;
}

#[tokio::test]
async fn cancelled_scan_task_does_not_begin_a_durable_generation() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let registry = Arc::new(Mutex::new(
        ImportFolderRegistry::from_stored(vec![root.to_string_lossy().into_owned()], Vec::new())
            .unwrap(),
    ));
    let (watch_tx, _watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = Arc::new(FolderWatcher::new(watch_tx));
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = service.event_tx.subscribe();

    let scan = spawn_root_scan(
        1,
        root,
        service.event_tx.clone(),
        service.library_manager.clone(),
        registry,
        Arc::new(tokio::sync::Mutex::new(())),
        watcher,
        completion_tx,
    );
    scan.cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(2), completion_rx.recv())
        .await
        .expect("cancelled scan did not report task completion")
        .expect("scan task completion channel closed");
    scan.task.await.unwrap();

    assert!(service
        .library_manager
        .load_folder_scan_snapshots()
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn coordinator_decision_waits_for_cancelled_scan_before_starting_replacement() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (
                crate::import::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    relative_folder_path: "Group".to_string(),
                },
                crate::import::FolderReleaseDecision::CombineAsOneRelease,
            ),
            completion: decision_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);
    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    assert!(!harness.scans.cancellation(1).is_cancelled());
    harness.scans.complete(1);
    assert_eq!(decision_result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_decision_validates_after_the_cancelled_scan_releases_its_commit() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;

    let commit = harness.folder_state_commit.clone().lock_owned().await;
    let (decision_completion, decision_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::SetFolderReleaseDecision {
            target: (
                crate::import::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_string_lossy().into_owned(),
                    relative_folder_path: "Group".to_string(),
                },
                crate::import::FolderReleaseDecision::CombineAsOneRelease,
            ),
            completion: decision_completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    harness
        .library_manager
        .remove_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap()
        .expect("the root was watched");
    drop(commit);

    assert_eq!(
        decision_result.await.unwrap(),
        Err("Group is not a current release boundary".to_string())
    );
    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_shutdown_waits_for_active_scan() {
    let harness = CoordinatorHarness::new().await;
    harness
        .commands
        .send(WatcherCommand::Rescan(root_path("/music")))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    let (shutdown_completion, shutdown_done) = std::sync::mpsc::channel();
    harness
        .commands
        .send(WatcherCommand::Shutdown {
            completion: shutdown_completion,
        })
        .unwrap();
    tokio::task::yield_now().await;
    assert!(matches!(
        shutdown_done.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    harness.scans.complete(0);
    tokio::task::spawn_blocking(move || shutdown_done.recv())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancelling_a_panicked_folder_walk_surfaces_the_join_failure() {
    async fn panic_during_walk() -> (
        Result<(), crate::import::folder_scanner::FolderScanError>,
        HashSet<PathBuf>,
    ) {
        panic!("folder walk panic");
    }

    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let (item_tx, mut item_rx) = tokio::sync::mpsc::channel(1);
    drop(item_tx);
    let error = ImportService::cancel_and_join_folder_walk(
        Path::new("/music"),
        &cancellation,
        &mut item_rx,
        tokio::spawn(panic_during_walk()),
    )
    .await
    .expect_err("a panicked traversal task cannot report a successful cancellation");

    assert!(cancellation.is_cancelled());
    assert!(error.to_string().contains("folder scan task failed"));
}

