//! What each watched root has going, driven through the coordinator: passes
//! asked for while one runs, removals and their failures, folder decisions,
//! and a folder taking over the watched folders inside it.

use super::*;
use crate::tables::watched_path::host_root;
use crate::watching::tests::*;
use bae_util::test_channels::HANG_GUARD;

#[tokio::test]
async fn coordinator_coalesces_same_root_to_one_followup_scan() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
    harness.commands.send(WatcherCommand::Rescan(root.clone())).unwrap();
    harness.commands.send(WatcherCommand::Rescan(root)).unwrap();
    // Both rescans are taken while the first pass runs; otherwise its end
    // could be taken first, leaving the root idle for one rescan to start a
    // pass and the other to queue a second.
    harness.commands_handled().await;
    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    assert_next_pass_reads_only_a_later_change(&harness, 2).await;
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_waits_for_the_active_scan_to_finish() {
    let harness = CoordinatorHarness::new().await;
    let mut result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;

    harness.commands_handled().await;
    assert!(
        harness.removal_backend.calls.lock().unwrap().is_empty(),
        "the watch was taken down while the scan could still install a late one"
    );
    assert!(
        !is_answered(&mut result),
        "removal completed while the scan could still install a late watch"
    );

    harness.scans.complete(0);
    assert_eq!(result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// A removal announces the folders its write left watched, whatever a read
/// after it would find.
#[tokio::test]
async fn a_removal_announces_the_folders_its_write_left_watched() {
    let harness = CoordinatorHarness::with_roots(&["/music", "/other"]).await;
    *harness.removal_backend.leaves_watched.lock().unwrap() = vec![root_path("/music")];
    *harness.removal_backend.unreadable_after_remove.lock().unwrap() =
        Some(harness.database.clone());
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove { roots: vec![root_path("/other")], parent: None, completion })
        .unwrap();
    assert_eq!(result.await.unwrap(), Ok(()));

    let mut events = harness.events.lock().await;
    let folders = loop {
        match tokio::time::timeout(HANG_GUARD, events.recv()).await.unwrap().unwrap() {
            crate::ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }) => {
                break folders
            }
            _ => continue,
        }
    };
    assert_eq!(
        folders.into_iter().map(|folder| folder.path).collect::<Vec<_>>(),
        [root_path("/music")]
    );
    drop(events);
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
            roots: vec![root_path("/music")],
            parent: None,
            completion: second_completion,
        })
        .unwrap();

    harness.scans.complete(0);
    assert_eq!(first_result.await.unwrap(), Ok(()));
    assert_eq!(second_result.await.unwrap(), Ok(()));
    assert_eq!(harness.removal_backend.calls.lock().unwrap().as_slice(), ["uninstall", "remove"]);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_blocked_root_removal_does_not_block_another_roots_refresh() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    let mut remove_result = removal_awaiting_its_cancelled_scan(&harness, "/music/one").await;

    let refresh_result = request_refresh(&harness, "/music/two");
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh_result)
            .await
            .expect("another root's refresh was blocked by removal")
            .unwrap(),
        Ok(())
    );

    harness.commands_handled().await;
    assert!(!is_answered(&mut remove_result), "removal completed before its blocked scan");
    harness.scans.complete(0);
    harness.shutdown().await;
}

/// A removal waits out its root's cancelled pass however the pass ended —
/// one that panicked was reported where it ran and never says it is over —
/// and then removes the root.
#[tokio::test]
async fn coordinator_removal_proceeds_once_its_cancelled_scan_ends_untold() {
    let harness = CoordinatorHarness::new().await;
    let result = removal_awaiting_its_cancelled_scan(&harness, "/music").await;
    harness.scans.abort(0);

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, result).await.expect("the removal is answered").unwrap(),
        Ok(())
    );
    assert_eq!(harness.removal_backend.calls.lock().unwrap().as_slice(), ["uninstall", "remove"]);
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
    assert_eq!(harness.removal_backend.calls.lock().unwrap().as_slice(), ["uninstall"]);
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

#[tokio::test]
async fn coordinator_removal_database_failure_reinstalls_and_rescans_before_returning() {
    let harness = CoordinatorHarness::new().await;
    let tables = crate::tables::ImportTables::new(harness.database.clone());
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
        tables.load_watched_import_folders().await.unwrap(),
        vec![crate::WatchedFolder::from_path(host_root("/music"))]
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
    harness.removal_backend.block_reinstall.store(true, std::sync::atomic::Ordering::SeqCst);
    let remove_result = removal_awaiting_its_cancelled_scan(&harness, "/music/one").await;
    harness.scans.complete(0);
    tokio::time::timeout(HANG_GUARD, harness.removal_backend.reinstall_started.notified())
        .await
        .expect("failed durable removal did not start watch restoration");

    let refresh_result = request_refresh(&harness, "/music/two");
    harness.scans.wait_for_count(2).await;
    // Asked once: the blocked reinstall holds whatever it holds until released.
    let mut other_root_commit = std::pin::pin!(harness.folder_state_commit.lock("hold for a test"));
    let other_root_commit = tokio::task::unconstrained(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(other_root_commit.as_mut(), cx))
    }))
    .await;
    let other_root_was_blocked = other_root_commit.is_pending();
    drop(other_root_commit);

    harness.removal_backend.release_reinstall.notify_one();
    harness.scans.complete(1);
    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh_result)
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
        harness.commands.send(WatcherCommand::Rescan(root_path(root))).unwrap();
    }
    harness.scans.wait_for_count(2).await;
    assert!(!harness.scans.cancellation(0).is_cancelled());
    assert!(!harness.scans.cancellation(1).is_cancelled());
    harness.scans.complete(0);
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A refresh waits for its scan to end, not to succeed: the scan reports its
/// own failure, and the caller reporting it too would show it twice.
#[tokio::test]
async fn coordinator_completes_refresh_waiter_once_its_scan_is_over() {
    let harness = CoordinatorHarness::new().await;
    let result = request_refresh(&harness, "/music");
    harness.scans.wait_for_count(1).await;
    harness.scans.complete(0);
    assert_eq!(result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// A decision cancels the whole-root pass it arrives during, runs once that
/// pass stops, and the root is read whole again because the pass left it half
/// read.
#[tokio::test]
async fn coordinator_decision_replaces_a_running_root_pass_and_owes_it_again() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
    let mut decision_result = request_group_decision(&harness, &root);
    harness.scans.wait_for_cancellation(0).await;
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    harness.scans.wait_for_count(2).await;
    assert_eq!(
        harness.scans.reading(1).map(|(key, _)| key.relative_folder),
        Some(crate::RelativeFolder::test("Group")),
        "the pass after the cancelled one is the folder reading"
    );
    // The reading is the fake pass's to answer now, and it has not.
    assert!(
        !is_answered(&mut decision_result),
        "the decision was answered before its reading was stored"
    );
    harness.scans.complete(1);
    assert_eq!(decision_result.await.unwrap(), Ok(Vec::new()));

    harness.scans.wait_for_count(3).await;
    assert!(harness.scans.reading(2).is_none(), "the root is read whole again");
    harness.scans.complete(2);
    harness.shutdown().await;
}

/// A decision on an idle root reads only its folder, a second decision waits
/// for the first rather than cancelling it, and the root is not read whole.
#[tokio::test]
async fn coordinator_decisions_on_an_idle_root_run_one_after_another() {
    let harness = CoordinatorHarness::new().await;
    let root = root_path("/music");
    let first = request_group_decision(&harness, &root);
    harness.scans.wait_for_count(1).await;
    assert!(harness.scans.reading(0).is_some());
    let second = request_group_decision(&harness, &root);
    harness.commands_handled().await;
    assert!(!harness.scans.cancellation(0).is_cancelled());
    assert_eq!(harness.scans.scans.lock().unwrap().len(), 1);

    harness.scans.complete(0);
    assert_eq!(first.await.unwrap(), Ok(Vec::new()));
    harness.scans.wait_for_count(2).await;
    assert!(harness.scans.reading(1).is_some());
    harness.scans.complete(1);
    assert_eq!(second.await.unwrap(), Ok(Vec::new()));
    assert_next_pass_reads_only_a_later_change(&harness, 2).await;
    harness.shutdown().await;
}

/// A decision still queued when its root is removed is told so rather than
/// left waiting.
#[tokio::test]
async fn coordinator_queued_decision_hears_its_root_is_being_removed() {
    let harness = CoordinatorHarness::new().await;
    let root = rescan_and_wait(&harness, "/music").await;
    let decision_result = request_group_decision(&harness, &root);
    harness.scans.wait_for_cancellation(0).await;
    let (completion, removal_result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove { roots: vec![root.clone()], parent: None, completion })
        .unwrap();

    assert_eq!(decision_result.await.unwrap(), Err(format!("{} is being removed", root.display())));
    harness.scans.complete(0);
    assert_eq!(removal_result.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

// A folder taking over the watched folders inside it: a removal of them with
// the folder watched in their place.

/// Hears whether a takeover landed.
type TakeoverAnswer = tokio::sync::oneshot::Receiver<Result<(), String>>;

/// Ask for `parent` to take over `inner`, and hear whether it landed.
fn request_takeover(harness: &CoordinatorHarness, parent: &str, inner: &[&str]) -> TakeoverAnswer {
    let (completion, result) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            roots: inner.iter().map(|root| root_path(root)).collect(),
            parent: Some(root_path(parent)),
            completion,
        })
        .unwrap();
    result
}

/// A pass over an inner folder is cancelled and waited out; then the parent
/// is read.
#[tokio::test]
async fn a_takeover_stops_the_inner_read_then_reads_the_parent() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;

    let mut taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;
    harness.commands_handled().await;
    assert!(
        harness.removal_backend.calls.lock().unwrap().is_empty(),
        "the takeover took the inner watches down while their pass still ran"
    );
    assert!(!is_answered(&mut taken_over), "the takeover waits for the pass it cancelled");

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "remove"]
    );
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.scans.lock().unwrap()[1].path, root_path("/music"));
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A refresh of an inner folder is answered by the parent's read.
#[tokio::test]
async fn a_refresh_during_a_takeover_waits_for_the_parent_read() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;
    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;
    let mut refreshed = request_refresh(&harness, "/music/two");
    harness.commands_handled().await;
    assert!(!is_answered(&mut refreshed), "the refresh waits for the takeover");

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    harness.scans.wait_for_count(2).await;
    assert_eq!(harness.scans.scans.lock().unwrap()[1].path, root_path("/music"));
    harness.commands_handled().await;
    assert!(!is_answered(&mut refreshed), "the refresh waits for the parent's read");
    harness.scans.complete(1);
    assert_eq!(refreshed.await.unwrap(), Ok(()));
    harness.shutdown().await;
}

/// Removing a folder that is being taken over is refused rather than raced
/// against the takeover.
#[tokio::test]
async fn removing_a_folder_being_taken_over_is_refused() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    rescan_and_wait(&harness, "/music/one").await;
    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);
    harness.scans.wait_for_cancellation(0).await;

    let (completion, removed) = tokio::sync::oneshot::channel();
    harness
        .commands
        .send(WatcherCommand::Remove {
            roots: vec![root_path("/music/two")],
            parent: None,
            completion,
        })
        .unwrap();
    assert_eq!(
        removed.await.unwrap(),
        Err(format!(
            "{} is being taken over by {}",
            root_path("/music/two").display(),
            root_path("/music").display()
        ))
    );

    harness.scans.complete(0);
    assert_eq!(taken_over.await.unwrap(), Ok(()));
    harness.scans.wait_for_count(2).await;
    harness.scans.complete(1);
    harness.shutdown().await;
}

/// A store change that does not land puts the watches back and reads the
/// inner folders again.
#[tokio::test]
async fn a_failed_takeover_restores_the_inner_folders() {
    let harness = CoordinatorHarness::with_roots(&["/music/one", "/music/two"]).await;
    *harness.removal_backend.remove_error.lock().unwrap() = Some("boom".to_string());

    let taken_over = request_takeover(&harness, "/music", &["/music/one", "/music/two"]);

    let error = taken_over.await.unwrap().unwrap_err();
    assert!(error.contains("boom"), "{error}");
    assert_eq!(
        harness.removal_backend.calls.lock().unwrap().as_slice(),
        ["uninstall", "uninstall", "remove", "reinstall", "reinstall"]
    );
    harness.scans.wait_for_count(2).await;
    let mut read_again: Vec<PathBuf> =
        harness.scans.scans.lock().unwrap().iter().map(|scan| scan.path.clone()).collect();
    read_again.sort();
    assert_eq!(read_again, vec![root_path("/music/one"), root_path("/music/two")]);
    for index in 0..2 {
        harness.scans.complete(index);
    }
    harness.shutdown().await;
}
