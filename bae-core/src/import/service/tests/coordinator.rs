/// Rescan `root` and wait for the coordinator to start its scan. Returns the
/// root in the host's own spelling, which the commands that follow address.
async fn rescan_and_wait(harness: &CoordinatorHarness, root: &str) -> PathBuf {
    let root = root_path(root);
    harness
        .commands
        .send(WatcherCommand::Rescan(root.clone()))
        .unwrap();
    harness.scans.wait_for_count(1).await;
    root
}

/// Where the removal tests start: `root` is being scanned, its removal has been
/// asked for, and the coordinator has cancelled the scan the removal waits on.
/// What that scan does next is the caller's, as is the returned removal result.
async fn removal_awaiting_its_cancelled_scan(
    harness: &CoordinatorHarness,
    root: &str,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let root = rescan_and_wait(harness, root).await;
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root,
            completion,
        })
        .unwrap();
    harness.scans.wait_for_cancellation(0).await;
    result
}

fn request_refresh(
    harness: &CoordinatorHarness,
    root: &str,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Refresh {
            path: root_path(root),
            completion,
        })
        .unwrap();
    result
}

/// Ask for `Group` under `root` to become one release.
fn request_group_decision(
    harness: &CoordinatorHarness,
    root: &Path,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (completion, result) = tokio::sync::oneshot::channel();
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
            completion,
        })
        .unwrap();
    result
}

#[tokio::test]
async fn coordinator_coalesces_same_root_to_one_followup_scan() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
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
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;

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
    let first_result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
    let (second_completion, second_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root_path("/music"),
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
    let remove_result = removal_awaiting_its_cancelled_scan(&harness, "/music/one").await;

    let refresh_result = request_refresh(&harness, "/music/two");
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
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
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
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
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
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
    harness.scans.complete(0);

    let error = result.await.unwrap().unwrap_err();
    assert!(error.contains("injected database failure"));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "remove", "reinstall"]
    );
    assert_eq!(
        harness
            .library_manager
            .load_watched_import_folders()
            .await
            .unwrap(),
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
    let remove_result = removal_awaiting_its_cancelled_scan(&harness, "/music/one").await;
    harness.scans.complete(0);
    tokio::time::timeout(
        Duration::from_secs(2),
        harness.removal_backend.reinstall_started.notified(),
    )
    .await
    .expect("failed durable removal did not start watch restoration");

    let refresh_result = request_refresh(&harness, "/music/two");
    harness.scans.wait_for_count(2).await;
    let other_root_commit = tokio::time::timeout(
        Duration::from_millis(50),
        harness.folder_state_commit.lock("hold for a test"),
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
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
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
    let result = request_refresh(&harness, "/music");
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
            watch_event(
                notify::EventKind::Access(AccessKind::Open(AccessMode::Any)),
                root_path("/music").join("Album/01.flac"),
            ),
            watch_event(
                notify::EventKind::Access(AccessKind::Close(AccessMode::Read)),
                root_path("/music").join("Album/01.flac"),
            ),
        ]))
        .unwrap();
    // A second batch the coordinator handles after the first, naming a root the
    // reads did not touch: when its scan starts, the read batch is answered.
    harness
        .fs_events
        .send(Ok(vec![watch_event(
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
/// finished copy announces itself on Linux, so it reads again the folder it
/// landed in — that folder, and nothing else under the root.
#[tokio::test]
async fn a_finished_write_under_a_watched_root_reads_its_folder_again() {
    use notify::event::{AccessKind, AccessMode};

    let harness = CoordinatorHarness::new().await;
    harness
        .fs_events
        .send(Ok(vec![watch_event(
            notify::EventKind::Access(AccessKind::Close(AccessMode::Write)),
            root_path("/music").join("Album/01.flac"),
        )]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root_path("/music"));
    assert_eq!(harness.scans.folders(0), Some(vec!["Album".to_string()]));

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// Changes that arrive while a folder is being read again are folded into
/// one reading afterwards: each changed folder once, whatever the burst.
#[tokio::test]
async fn changes_during_a_folder_reading_are_read_once_afterwards() {
    let harness = CoordinatorHarness::new().await;
    let change = |folder: &str, file: &str| {
        Ok(vec![watch_event(
            notify::EventKind::Create(notify::event::CreateKind::File),
            root_path("/music").join(folder).join(file),
        )])
    };
    harness.fs_events.send(change("One", "01.flac")).unwrap();
    harness.scans.wait_for_count(1).await;
    for (folder, file) in [("Two", "01.flac"), ("Two", "02.flac"), ("One", "03.flac")] {
        harness.fs_events.send(change(folder, file)).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    assert_eq!(
        harness.scans.folders(1),
        Some(vec!["One".to_string(), "Two".to_string()])
    );
    harness.scans.complete(1);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 2);
    harness.shutdown().await;
}

/// A watch that says it lost track of changes and names no path — inotify's
/// queue overflowing — could have missed anything, so every root is read
/// whole.
#[tokio::test]
async fn a_watch_that_lost_track_of_no_path_reads_the_root_whole() {
    let harness = CoordinatorHarness::new().await;
    harness.fs_events.send(Ok(vec![lost_track(None)])).unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root_path("/music"));
    assert_eq!(harness.scans.folders(0), None);
    assert!(harness.scans.reading(0).is_none(), "the root is read whole");

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// A watch that lost track naming a path that holds the root — FSEvents
/// dropping events for the whole stream — reads the root whole.
#[tokio::test]
async fn a_watch_that_lost_track_above_the_root_reads_it_whole() {
    let harness = CoordinatorHarness::new().await;
    let above = root_path("/music").parent().unwrap().to_path_buf();
    harness
        .fs_events
        .send(Ok(vec![lost_track(Some(above))]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.folders(0), None);
    assert!(harness.scans.reading(0).is_none(), "the root is read whole");

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// A watch that lost track inside one album — FSEvents' must-scan-subdirs
/// naming that folder — reads that album's folder again and nothing beside
/// it.
#[tokio::test]
async fn a_watch_that_lost_track_inside_one_folder_reads_only_that_folder() {
    let harness = CoordinatorHarness::new().await;
    harness
        .fs_events
        .send(Ok(vec![lost_track(Some(
            root_path("/music").join("Artist").join("Album"),
        ))]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root_path("/music"));
    assert_eq!(harness.scans.folders(0), Some(vec!["Artist".to_string()]));

    harness.scans.complete(0);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);
    harness.shutdown().await;
}

fn watch_event(kind: notify::EventKind, path: PathBuf) -> notify::Event {
    notify::Event::new(kind).add_path(path)
}

/// What a watch that lost track reports: a rescan-flagged event naming where
/// to start reading again, or nothing at all.
fn lost_track(path: Option<PathBuf>) -> notify::Event {
    let event = notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
    match path {
        Some(path) => event.add_path(path),
        None => event,
    }
}

#[tokio::test]
async fn coordinator_completes_scan_while_filesystem_batches_remain_ready() {
    let harness = CoordinatorHarness::new().await;
    let result = request_refresh(&harness, "/music");
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
    let TestService {
        service,
        preparations,
        temp: tmp,
    } = setup_import_service().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let (watch_tx, _watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let watcher = Arc::new(FolderWatcher::new(watch_tx));
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut events = service.event_tx.subscribe();

    let scan = spawn_root_scan(
        1,
        root,
        test_scan_services(
            &service,
            &preparations,
            service.event_tx.clone(),
            watcher,
            std::sync::Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
            std::sync::Arc::new(crate::import::folder_scanner::OsDirectoryReader),
        ),
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

/// A decision replaces the whole-root pass it arrives during: that pass is
/// cancelled, the folder reading runs once it has stopped, and the root is
/// read whole again afterwards because the cancelled pass left it half read.
#[tokio::test]
async fn coordinator_decision_replaces_a_running_root_pass_and_owes_it_again() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
    let mut decision_result = Box::pin(request_group_decision(&harness, &root));
    harness.scans.wait_for_cancellation(0).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    assert_eq!(
        harness.scans.reading(1).map(|(key, _)| key.relative_folder_path),
        Some("Group".to_string()),
        "the pass after the cancelled one is the folder reading"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), decision_result.as_mut())
            .await
            .is_err(),
        "the decision was answered before its reading was stored"
    );
    harness.scans.complete(1);
    assert_eq!(decision_result.await.unwrap(), Ok(()));

    harness.scans.wait_for_count(3).await;
    assert!(harness.scans.reading(2).is_none(), "the root is read whole again");
    harness.scans.complete(2);
    harness.shutdown().await;
}

/// A decision on an idle root reads only its folder, and a second decision
/// waits for the first rather than cancelling it. Nothing reads the whole
/// root afterwards: nothing asked for that.
#[tokio::test]
async fn coordinator_decisions_on_an_idle_root_run_one_after_another() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    let first = request_group_decision(&harness, &root);
    harness.scans.wait_for_count(1).await;
    assert!(harness.scans.reading(0).is_some());
    let second = request_group_decision(&harness, &root);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!harness.scans.cancellation(0).is_cancelled());
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    assert_eq!(first.await.unwrap(), Ok(()));
    harness.scans.wait_for_count(2).await;
    assert!(harness.scans.reading(1).is_some());
    harness.scans.complete(1);
    assert_eq!(second.await.unwrap(), Ok(()));
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 2);
    harness.shutdown().await;
}

/// A decision still queued when its root is removed hears that, rather than
/// waiting on a reading that will never run.
#[tokio::test]
async fn coordinator_queued_decision_hears_its_root_is_being_removed() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
    let decision_result = request_group_decision(&harness, &root);
    harness.scans.wait_for_cancellation(0).await;
    let (completion, removal_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            path: root.clone(),
            completion,
        })
        .unwrap();

    assert_eq!(
        decision_result.await.unwrap(),
        Err(format!("{} is being removed", root.display()))
    );
    harness.scans.complete(0);
    assert_eq!(removal_result.await.unwrap(), Ok(()));
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
    async fn panic_during_walk() -> super::FolderWalkOutcome {
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
