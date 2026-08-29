use super::live_query_tests::{candidate_names, list_request, live_db, scan_candidate};
use std::time::Duration;

/// The list is a live query over the scan tables, so a scan item written while
/// someone is watching wakes it. A candidate's rows span several tables now; a
/// dependency missed on any of them would leave the list showing the previous
/// scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::folder_registry::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scan_candidate(root, "first"))
        .await
        .unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);
    let initial_scan = initial
        .summary
        .folder_scan_activity
        .as_ref()
        .expect("the open generation projects scan activity");
    assert_eq!(initial_scan.found_count, 1);
    assert_eq!(initial_scan.folders[0].found_count, 1);

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
    let grown_scan = grown
        .summary
        .folder_scan_activity
        .expect("the scan activity updates with the current generation");
    assert_eq!(grown_scan.found_count, 2);
    assert_eq!(grown_scan.folders[0].found_count, 2);

    db.finish_folder_scan(root, generation, None).await.unwrap();
    let complete = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("finishing the scan wakes the list")
        .into_result()
        .unwrap();
    assert_eq!(complete.summary.folder_scan_activity, None);
}

/// A rescan retains the previous generation until the new walk succeeds, but
/// progress counts only the entries encountered by the generation in flight.
#[tokio::test]
async fn import_list_scan_activity_excludes_retained_previous_generation_rows() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::folder_registry::host_root("/music");
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

    let projection = db
        .load_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]))
        .await
        .unwrap();
    let activity = projection
        .summary
        .folder_scan_activity
        .expect("the rescan projects activity");
    assert_eq!(activity.found_count, 1);
    assert_eq!(activity.folders[0].found_count, 1);
}
