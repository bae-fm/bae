use super::import_list_live_query_tests::{candidate_names, list_request, scan_candidate};
use super::live_query_tests::{live_db, next_run_is_only_the_request};
use std::time::Duration;

/// The list is a live query over the scan tables, so a scan item written while
/// someone is watching wakes it. A candidate's rows span several tables now; a
/// dependency missed on any of them would leave the list showing the previous
/// scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &scan_candidate(root, "first"))
        .await
        .unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    db.save_folder_scan_item(root, generation, &scan_candidate(root, "second"))
        .await
        .unwrap();
    let grown = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a scan item wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        candidate_names(&grown),
        vec!["first".to_string(), "second".to_string()]
    );
    assert_eq!(grown.total_count, 2);
}

/// The scan progress counts the folders the walk in flight has reached, and
/// is gone once the walk finishes.
#[tokio::test]
async fn folder_scan_progress_follows_the_walk_in_flight() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &scan_candidate(root, "first"))
        .await
        .unwrap();

    let mut live = db.subscribe_folder_scan_progress();
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the open generation projects scan activity");
    assert_eq!(initial.found_count, 1);
    assert_eq!(initial.folders[0].found_count, 1);

    db.save_folder_scan_item(root, generation, &scan_candidate(root, "second"))
        .await
        .unwrap();
    let grown = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a scan item wakes the scan progress")
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the scan activity updates with the current generation");
    assert_eq!(grown.found_count, 2);
    assert_eq!(grown.folders[0].found_count, 2);

    db.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("finishing the scan wakes the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(complete.activity, None);
    assert!(matches!(
        complete.statuses[0].status,
        crate::import::FolderScanStatus::Complete
    ));
}

/// Whether a watched folder is on a network volume is what its scan recorded
/// when it began — not something reading the progress asks the filesystem,
/// which for a share that stopped answering would wait as long as the mount.
/// A folder on this machine's disk recorded as network reads as network.
#[tokio::test]
async fn folder_scan_progress_reads_the_volume_its_scan_recorded() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Network)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let progress = db
        .subscribe_folder_scan_progress()
        .next()
        .await
        .expect("the query stays open")
        .unwrap();

    assert!(progress.statuses[0].on_network_volume);
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// progress counts only the entries encountered by the generation in flight.
#[tokio::test]
async fn folder_scan_progress_excludes_retained_previous_generation_rows() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let old_generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["retained", "encountered"] {
        db.save_folder_scan_item(root, old_generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, old_generation, None)
        .await
        .unwrap();

    let current_generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(
        root,
        current_generation,
        &scan_candidate(root, "encountered"),
    )
    .await
    .unwrap();

    let activity = db
        .subscribe_folder_scan_progress()
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .activity
        .expect("the rescan projects activity");
    assert_eq!(activity.found_count, 1);
    assert_eq!(activity.folders[0].found_count, 1);
}

/// A rescan re-confirms every folder it walks by moving its row to the new
/// generation, and on an unchanged folder that is all it writes. Nothing the
/// list shows moves with it, so the list stays asleep: a whole-queue read per
/// re-confirmed folder kept the list reading back to back for as long as a
/// startup rescan of a large folder lasted.
#[tokio::test]
async fn import_list_sleeps_through_a_rescan_that_reconfirms_its_candidates() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        candidate_names(&initial),
        vec!["first".to_string(), "second".to_string()]
    );

    for name in ["first", "second"] {
        db.save_folder_scan_item(root, rescan, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    let moved = next_run_is_only_the_request(
        &mut live,
        list_request(crate::import::TriageTab::Pending, [(0, 49)]),
    )
    .await
    .result
    .unwrap();
    assert_eq!(moved.windows[0].items, initial.windows[0].items);
}

/// A scan's progress moves the folder scans' own query and sends the list
/// subscription nothing: the next list value after it is the one a new
/// request asks for, with the rows its last read held.
#[tokio::test]
async fn a_scans_progress_moves_the_scans_and_sends_the_list_nothing() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();

    let request = list_request(crate::import::TriageTab::Pending, [(0, 50)]);
    let (_outbox, outbox) = tokio::sync::watch::channel(None);
    let subscription = crate::import::ImportListSubscription::start(
        |request| db.subscribe_import_list(request),
        request.view,
        request.windows,
        outbox,
        &tokio::runtime::Handle::current(),
    );
    let mut scans = db.subscribe_folder_scan_progress();
    let initial = subscription.next().await.unwrap();
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    let found = |progress: crate::import::FolderScanProgress| {
        progress
            .activity
            .expect("the rescan is in flight")
            .found_count
    };
    assert_eq!(
        found(scans.next().await.expect("the query stays open").unwrap()),
        0
    );

    db.save_folder_scan_item(root, rescan, &scan_candidate(root, "first"))
        .await
        .unwrap();
    let progressed = tokio::time::timeout(Duration::from_secs(2), scans.next())
        .await
        .expect("the re-confirmed folder moves the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(found(progressed), 1);

    subscription
        .set_windows(
            std::iter::once(crate::library::LibraryPageWindow {
                offset: 0,
                limit: 49,
            })
            .collect(),
        )
        .unwrap();
    let next = tokio::time::timeout(Duration::from_secs(2), subscription.next())
        .await
        .expect("a new request reruns the list")
        .unwrap();
    assert_eq!(
        next.cause,
        coven::LiveQueryCause::Request,
        "the scan's progress sent the list nothing"
    );
    assert_eq!(next.windows[0].items, initial.windows[0].items);
}
