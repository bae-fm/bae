use super::import_list_live_query_tests::{candidate_names, list_request, through};
use super::scan_candidate;
use bae_db::test_fixtures::live_query::{live_db, next_run_is_only_the_request};
use std::time::Duration;

/// The list is a live query over the scan tables, so a scan item written while
/// someone is watching wakes it. A candidate's rows span several tables now; a
/// dependency missed on any of them would leave the list showing the previous
/// scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = live_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &bae_db::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a scan item wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(candidate_names(&grown), vec!["first".to_string(), "second".to_string()]);
    assert_eq!(grown.total_count, 2);
}

/// A rescan re-confirms every folder it walks by moving its row to the new
/// generation, and on an unchanged folder that is all it writes. Nothing the
/// list shows moves with it, so the list stays asleep: a whole-queue read per
/// re-confirmed folder kept the list reading back to back for as long as a
/// startup rescan of a large folder lasted.
#[tokio::test]
async fn import_list_sleeps_through_a_rescan_that_reconfirms_its_candidates() {
    let (db, _temp) = live_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &bae_db::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, generation, &scan_candidate(root, name)).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();

    let request = list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await;
    let list = request.list;
    let mut live = tables.subscribe_import_list(request);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string(), "second".to_string()]);

    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, rescan, &scan_candidate(root, name)).await.unwrap();
    }
    let moved =
        next_run_is_only_the_request(&mut live, through(list, [(0, 49)])).await.result.unwrap();
    assert_eq!(moved.windows[0].items, initial.windows[0].items);
}

/// A scan's progress moves the folder scans' own query and sends the list
/// subscription nothing: the next list value after it is the one a new
/// request asks for, with the rows its last read held.
#[tokio::test]
async fn a_scans_progress_moves_the_scans_and_sends_the_list_nothing() {
    let (db, _temp) = live_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &bae_db::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, generation, &scan_candidate(root, name)).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let rescan = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();

    let request = list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await;
    let list = request.list;
    let mut subscription = tables.subscribe_import_list(request);
    let requests = subscription.requests();
    let mut scans = tables.subscribe_folder_scan_progress();
    let initial = subscription.next().await.expect("the query stays open");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    let found = |progress: crate::FolderScanProgress| {
        progress.activity.expect("the rescan is in flight").found_count
    };
    assert_eq!(found(scans.next().await.expect("the query stays open").unwrap()), 0);

    tables.save_folder_scan_item(root, rescan, &scan_candidate(root, "first")).await.unwrap();
    let progressed = tokio::time::timeout(Duration::from_secs(2), scans.next())
        .await
        .expect("the re-confirmed folder moves the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(found(progressed), 1);

    requests.set(through(list, [(0, 49)])).unwrap();
    let next = tokio::time::timeout(Duration::from_secs(2), subscription.next())
        .await
        .expect("a new request reruns the list")
        .expect("the query stays open");
    assert_eq!(
        next.cause,
        coven::LiveQueryCause::Request,
        "the scan's progress sent the list nothing"
    );
    assert_eq!(next.result.unwrap().windows[0].items, initial.result.unwrap().windows[0].items);
}
