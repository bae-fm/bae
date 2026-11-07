//! The coordinator's loop: what a watch report asks to have read, a watch
//! that lost track, the network check, and its shutdown.

use super::*;
use crate::service::tests::*;
use crate::watching::tests::*;
use bae_util::test_channels::HANG_GUARD;

/// A read is not a change: Linux reports the scan's own reads as events, and
/// scanning on them would make every scan schedule the next.
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
    // Handled after the first batch, so once its scan starts the reads have
    // been seen.
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

/// A close that ended a write, which is how a finished copy shows up on Linux,
/// reads again the folder it landed in and nothing else under the root.
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
    assert_eq!(harness.scans.folders(0), Some(vec!["Album".into()]));

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// Changes that arrive during a folder reading are read once afterwards, each
/// changed folder once.
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
        harness.watch_report(change(folder, file)).await;
    }
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.folders(1), Some(vec!["One".into(), "Two".into()]));
    harness.scans.complete(1);
    assert_next_pass_reads_only_a_later_change(&harness, 2).await;
    harness.shutdown().await;
}

/// A watch that lost track of changes without naming a path (inotify's queue
/// overflowing) could have missed anything, so the root is read whole.
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

/// A watch that lost track above the root (FSEvents dropping events for the
/// whole stream) reads the root whole.
#[tokio::test]
async fn a_watch_that_lost_track_above_the_root_reads_it_whole() {
    let harness = CoordinatorHarness::new().await;
    let above = root_path("/music").parent().unwrap().to_path_buf();
    harness.fs_events.send(Ok(vec![lost_track(Some(above))])).unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.folders(0), None);
    assert!(harness.scans.reading(0).is_none(), "the root is read whole");

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// A watch that lost track inside one album (FSEvents' must-scan-subdirs)
/// reads again only the top-level folder holding it.
#[tokio::test]
async fn a_watch_that_lost_track_inside_one_folder_reads_only_that_folder() {
    let harness = CoordinatorHarness::new().await;
    harness
        .fs_events
        .send(Ok(vec![lost_track(Some(root_path("/music").join("Artist").join("Album")))]))
        .unwrap();

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root_path("/music"));
    assert_eq!(harness.scans.folders(0), Some(vec!["Artist".into()]));

    harness.scans.complete(0);
    assert_next_pass_reads_only_a_later_change(&harness, 1).await;
    harness.shutdown().await;
}

/// A rescan-flagged event, as a watch that lost track reports, naming where to
/// read again or nothing.
fn lost_track(path: Option<PathBuf>) -> notify::Event {
    let event = notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
    match path {
        Some(path) => event.add_path(path),
        None => event,
    }
}

/// A pass's end is taken ahead of watch reports that are already waiting.
/// A rescan of another root is taken ahead of both, and the coordinator is
/// held starting its pass while a hundred reports, each naming a folder of its
/// own under `/music`, queue up ahead of `/music`'s pass end. Had it taken any
/// report first, that folder would wait on the running pass and be read by the
/// pass its end starts; it takes the end first, leaving the root idle, so the
/// first report starts a pass that reads its folder alone.
#[tokio::test]
async fn coordinator_completes_scan_while_filesystem_batches_remain_ready() {
    let harness = CoordinatorHarness::with_roots(&["/music", "/other"]).await;
    let music = root_path("/music");
    let result = request_refresh(&harness, "/music");
    harness.scans.wait_for_count(1).await;

    let held = harness.scans.hold_starts();
    harness.commands.send(WatcherCommand::Rescan(root_path("/other"))).unwrap();
    for folder in 0..100 {
        harness
            .fs_events
            .send(Ok(vec![watch_event(
                notify::EventKind::Create(notify::event::CreateKind::File),
                music.join(format!("Folder {folder:03}")).join("01.flac"),
            )]))
            .unwrap();
    }
    harness.scans.complete(0);
    drop(held);

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, result).await.expect("the refresh is answered").unwrap(),
        Ok(())
    );
    harness.scans.wait_for_count(3).await;
    assert_eq!(harness.scans.path(1), root_path("/other"));
    assert_eq!(harness.scans.path(2), music);
    assert_eq!(
        harness.scans.folders(2),
        Some(vec!["Folder 000".into()]),
        "no report was taken ahead of the pass end"
    );
    // Ended without telling the coordinator, so the reports still waiting
    // start no further pass for the shutdown to wait on.
    harness.scans.abort(1);
    harness.scans.abort(2);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_shutdown_waits_for_active_scan() {
    let harness = CoordinatorHarness::new().await;
    harness.commands.send(WatcherCommand::Rescan(root_path("/music"))).unwrap();
    harness.scans.wait_for_count(1).await;
    let (shutdown_completion, shutdown_done) = std::sync::mpsc::channel();
    harness.commands.send(WatcherCommand::Shutdown { completion: shutdown_completion }).unwrap();
    // Shutdown cancels the scan before it waits for it, and only the scan
    // ending lets it finish.
    harness.scans.wait_for_cancellation(0).await;
    assert!(matches!(shutdown_done.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
    harness.scans.complete(0);
    bae_util::test_blocking::on_blocking_pool(move || shutdown_done.recv()).await.unwrap();
}

/// A network folder whose check finds it as its last scan left it is checked
/// again: the check answers whatever it finds, and its answer frees the
/// folder for the next check.
#[tokio::test]
async fn a_network_folder_found_unchanged_is_checked_again() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("watched");
    std::fs::create_dir(&root).unwrap();
    let modified_at =
        crate::volume::directory_modified_at(&root).expect("the folder's mtime reads");
    tables.add_watched_import_folder(&root).await.unwrap();
    tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.record_folder_scan_directories(&root, &[(root.clone(), modified_at)]).await.unwrap();
    let (_scans, starter) = FakeScanStarter::new();
    let (scan_completions, _scan_completions) = tokio::sync::mpsc::unbounded_channel();
    let (removal_completions, _removal_completions) = tokio::sync::mpsc::unbounded_channel();
    let active_roots = ActiveRoots::new(
        starter,
        Arc::new(FakeRemovalBackend::default()),
        crate::FolderStateCommit::new(),
        bae_diagnostics::Diagnostics::noop(),
        scan_completions,
        removal_completions,
        tokio::sync::watch::channel(crate::folder_scan_status::FolderScansInMemory::new()).0,
    );
    let (checked_tx, mut checks) = tokio::sync::mpsc::unbounded_channel();
    let mut coordinator = crate::watching::coordinator::Coordinator::new(
        active_roots,
        test.event_tx().clone(),
        tables,
        bae_diagnostics::Diagnostics::noop(),
        checked_tx,
    );

    for check in ["first", "second"] {
        assert!(coordinator.start_network_check(root.clone()), "the {check} check starts");
        assert!(
            !coordinator.start_network_check(root.clone()),
            "one check of the folder runs at a time"
        );
        let (checked, found) = tokio::time::timeout(HANG_GUARD, checks.recv())
            .await
            .expect("the check answers")
            .expect("the coordinator still hears checks");
        assert_eq!(checked, root);
        assert_eq!(found, crate::watching::coordinator::NetworkCheck::Unchanged);
        coordinator
            .take(crate::watching::coordinator::CoordinatorEvent::NetworkChecked {
                root: checked,
                found,
            })
            .await;
    }
}

/// Every check period on the coordinator's clock, each folder whose last scan
/// found it on a network volume is checked against what that scan saw, and
/// what moved is read again; the next check is armed one period on.
#[tokio::test]
async fn a_network_folder_is_checked_once_the_check_period_passes() {
    let start = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
    let clock = Arc::new(coven::FixedClock::new(start));
    let harness = CoordinatorHarness::with_roots_on(&[], clock.clone()).await;
    let (_, armed) = tokio::time::timeout(HANG_GUARD, clock.sleep_begun_after(0))
        .await
        .expect("the coordinator arms its first check");
    assert_eq!(armed, crate::volume::CHECK_PERIOD);

    let folders = tempfile::TempDir::new().unwrap();
    let root = folders.path().join("watched");
    let album = root.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let root_modified_at =
        crate::volume::directory_modified_at(&root).expect("the folder's mtime reads");
    let tables = crate::tables::ImportTables::new(harness.db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    tables.begin_folder_scan(&root, crate::VolumeKind::Network).await.unwrap();
    // The album changed after the scan recorded it.
    tables
        .record_folder_scan_directories(&root, &[(root.clone(), root_modified_at), (album, 0)])
        .await
        .unwrap();

    clock.set(start + crate::volume::CHECK_PERIOD);

    harness.scans.wait_for_count(1).await;
    assert_eq!(harness.scans.path(0), root);
    assert_eq!(harness.scans.folders(0), Some(vec!["Album".into()]));
    let (_, rearmed) = tokio::time::timeout(HANG_GUARD, clock.sleep_begun_after(1))
        .await
        .expect("the coordinator arms the next check");
    assert_eq!(rearmed, crate::volume::CHECK_PERIOD * 2);

    harness.scans.complete(0);
    harness.shutdown().await;
}

/// A failed read of the watched folders is not "nothing is watched": the
/// report's changes are not read, and the failure is reported as a bug.
#[tokio::test]
async fn a_watch_report_over_unreadable_watched_folders_reports_the_failure() {
    let harness = CoordinatorHarness::new().await;
    harness.corrupt_watched_folders().await;
    harness
        .watch_report(Ok(vec![watch_event(
            notify::EventKind::Create(notify::event::CreateKind::File),
            root_path("/music").join("Album/01.flac"),
        )]))
        .await;

    let reports = harness.bugs.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(reports[0].1.contains("two overlapping stored watched folders"), "{reports:?}");
    harness.shutdown().await;
}

/// A rescan of every watched folder over an unreadable list fails to the one
/// who asked, and starts no scan.
#[tokio::test]
async fn a_rescan_of_unreadable_watched_folders_fails_to_its_caller() {
    let harness = CoordinatorHarness::new().await;
    harness.corrupt_watched_folders().await;
    let (completion, answer) = tokio::sync::oneshot::channel();
    harness.commands.send(WatcherCommand::RescanAll { completion }).unwrap();

    let error = tokio::time::timeout(HANG_GUARD, answer)
        .await
        .expect("the coordinator answers")
        .expect("the coordinator is running")
        .expect_err("the unreadable list fails the rescan");
    assert!(error.to_string().contains("two overlapping stored watched folders"), "{error}");
    harness.commands_handled().await;
    assert!(harness.scans.scans.lock().unwrap().is_empty());
    harness.shutdown().await;
}

/// A failed read of whether a folder is watched is not "it is not watched":
/// the rescan asked for is not dropped quietly, the failure is reported.
#[tokio::test]
async fn a_rescan_that_cannot_tell_if_its_folder_is_watched_reports_the_failure() {
    let harness = CoordinatorHarness::new().await;
    harness.db.close().await.unwrap();
    harness.commands.send(WatcherCommand::Rescan(root_path("/music"))).unwrap();
    harness.commands_handled().await;

    let reports = harness.bugs.reports();
    assert!(reports.iter().any(|(operation, _)| operation.contains("rescan")), "{reports:?}");
    harness.shutdown().await;
}

/// A network folder's check that cannot read what the last scan saw ends
/// unfinished and reports it, rather than comparing against nothing.
#[tokio::test]
async fn a_network_check_that_cannot_read_the_last_scan_reports_the_failure() {
    let harness = CoordinatorHarness::new().await;
    crate::tables::ImportTables::lose_scan_directories_for_test(harness.library.path()).await;
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let tables = crate::tables::ImportTables::new(harness.db.clone());

    let found = check_network_folder(&tables, &root_path("/music"), &diagnostics).await;

    assert_eq!(found, NetworkCheck::Unfinished);
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
    harness.shutdown().await;
}
