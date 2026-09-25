use super::import_list_live_query_tests::{candidate_names, list_request, scan_candidate};
use super::live_query_tests::live_db;
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
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scan_candidate(root, "first"))
        .await
        .unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    db.save_folder_scan_item(root, generation, &scan_candidate(root, "second"))
        .await
        .unwrap();
    let grown = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a scan item wakes the list")
        .into_result()
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
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scan_candidate(root, "first"))
        .await
        .unwrap();

    let mut live = db.subscribe_folder_scan_progress();
    let initial = live
        .next()
        .await
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
        .unwrap()
        .activity
        .expect("the scan activity updates with the current generation");
    assert_eq!(grown.found_count, 2);
    assert_eq!(grown.folders[0].found_count, 2);

    db.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("finishing the scan wakes the scan progress")
        .unwrap();
    assert_eq!(complete.activity, None);
    assert!(matches!(
        complete.statuses[0].status,
        crate::import::FolderScanStatus::Complete
    ));
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// progress counts only the entries encountered by the generation in flight.
#[tokio::test]
async fn folder_scan_progress_excludes_retained_previous_generation_rows() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let old_generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["retained", "encountered"] {
        db.save_folder_scan_item(root, old_generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, old_generation, None)
        .await
        .unwrap();

    let current_generation = db.begin_folder_scan(root).await.unwrap();
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
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = db.begin_folder_scan(root).await.unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(
        candidate_names(&initial),
        vec!["first".to_string(), "second".to_string()]
    );

    for name in ["first", "second"] {
        db.save_folder_scan_item(root, rescan, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(500), live.next())
            .await
            .is_err(),
        "re-confirming unchanged folders delivers no list value"
    );
}

/// The list subscription delivers a scan's progress beside the rows it
/// already read: the count moves, and the list's revision and rows stay what
/// its last read answered.
#[tokio::test]
async fn import_list_subscription_delivers_scan_progress_beside_its_last_read() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = db.begin_folder_scan(root).await.unwrap();

    let request = list_request(crate::import::TriageTab::Pending, [(0, 50)]);
    let (_outbox, outbox) = tokio::sync::watch::channel(None);
    let subscription = crate::import::ImportListSubscription::start(
        db.subscribe_import_list(request.clone()),
        db.subscribe_folder_scan_progress(),
        request,
        outbox,
        &tokio::runtime::Handle::current(),
    );
    let initial = subscription.next().await.unwrap();
    assert_eq!(initial.cause, coven::ReconfigurableLiveQueryCause::Initial);
    assert_eq!(
        initial
            .folder_scans
            .activity
            .expect("the rescan is in flight")
            .found_count,
        0
    );

    db.save_folder_scan_item(root, rescan, &scan_candidate(root, "first"))
        .await
        .unwrap();
    let progressed = tokio::time::timeout(Duration::from_secs(2), subscription.next())
        .await
        .expect("the re-confirmed folder moves the scan progress")
        .unwrap();
    assert_eq!(
        progressed.cause,
        coven::ReconfigurableLiveQueryCause::DatabaseChanged
    );
    assert_eq!(progressed.request_revision, initial.request_revision);
    assert_eq!(progressed.total_count, 2);
    assert_eq!(
        progressed
            .folder_scans
            .activity
            .expect("the rescan is in flight")
            .found_count,
        1
    );
}
