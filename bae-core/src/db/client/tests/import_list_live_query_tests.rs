//! The import list's live query: what reruns it, what it withholds, and how
//! Done follows what this device imported.

use super::super::*;
use super::exec;
use super::live_query_tests::{live_db, next_run_is_only_the_request, ALBUM_ID, RELEASE_ID};
use std::time::Duration;
/// The import list's request carries the view and the windows, so every test
/// below states both.
/// A list opened on `tab` in path order, read through `windows`.
pub(super) async fn list_request(
    db: &Database,
    tab: crate::import::TriageTab,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::import::ImportListRequest {
    let list = db
        .open_import_list(crate::import::ImportListView {
            tab,
            order: crate::import::ImportListOrder::PathAscending,
            ..crate::import::ImportListView::default()
        })
        .await
        .unwrap();
    through(list, windows)
}

/// The list `list` read through `windows`.
pub(super) fn through(
    list: crate::import::ImportListId,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::import::ImportListRequest {
    crate::import::ImportListRequest {
        list,
        windows: windows
            .into_iter()
            .map(|(offset, limit)| crate::library::LibraryPageWindow { offset, limit })
            .collect(),
    }
}

pub(super) fn scan_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    crate::import::folder_scanner::ScanItem::Valid(super::candidate(root, name))
}

pub(super) fn candidate_names(projection: &crate::import::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

/// Store `name`'s candidate under `root` as one finished scan of it, and
/// return it.
async fn scanned(
    db: &Database,
    root: &str,
    name: &str,
) -> crate::import::folder_scanner::FolderCandidate {
    let item = scan_candidate(root, name);
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let candidate = candidate.clone();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    candidate
}

/// What an import here records beside the release it writes: the folder hash
/// of the files it read.
async fn record_import(db: &Database, content_hash: &str) {
    exec(
        db,
        "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
        &[RELEASE_ID, content_hash],
    )
    .await;
}

/// The candidate rows one read of the list holds.
fn rows(projection: &crate::import::ImportListProjection) -> Vec<crate::import::TriageRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_this_device_imports_its_files() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let candidate = scanned(&db, root, "release").await;

    let mut live = db.subscribe_import_list(
        list_request(&db, crate::import::TriageTab::Pending, [(0, 50)]).await,
    );
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.summary.counts.pending, 1);
    assert_eq!(initial.summary.counts.done, 0);

    record_import(&db, &candidate.files.content_hash()).await;

    let imported = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the recorded import wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(imported.total_count, 0, "Pending no longer holds the row");
    assert_eq!(imported.summary.counts.done, 1);
}

/// A Done row is the row the folder was in Found — the same reading, cover
/// and text — placed Done, its import status naming the one release this
/// device imported from its files.
#[tokio::test]
async fn a_done_row_is_the_row_it_was_in_found() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let candidate = scanned(&db, root, "release").await;
    let found = rows(
        &db.load_import_list(list_request(&db, crate::import::TriageTab::Pending, [(0, 50)]).await)
            .await
            .unwrap(),
    );
    assert_eq!(found.len(), 1);

    record_import(&db, &candidate.files.content_hash()).await;
    let done = rows(
        &db.load_import_list(list_request(&db, crate::import::TriageTab::Done, [(0, 50)]).await)
            .await
            .unwrap(),
    );
    assert_eq!(done.len(), 1);
    let done = &done[0];
    assert_eq!(done.placement, crate::import::TriagePlacement::Done);
    assert_eq!(
        done.import_status,
        Some(crate::import::TriageImportStatus::Complete {
            release: crate::import::ImportedRelease {
                release_id: RELEASE_ID.to_string(),
                album_id: ALBUM_ID.to_string(),
            },
        })
    );
    assert_eq!(
        crate::import::TriageRow {
            placement: found[0].placement,
            action_basis: found[0].action_basis.clone(),
            import_status: None,
            ..done.clone()
        },
        found[0],
        "everything the row shows is what it showed in Found"
    );
}

/// A release in the library that this device did not import — one another
/// device imported from the same files and synced here — leaves the folder in
/// Found like any other.
#[tokio::test]
async fn a_release_this_device_did_not_import_leaves_the_folder_in_found() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let candidate = scanned(&db, root, "release").await;
    assert!(db.find_release_by_id(RELEASE_ID).await.unwrap().is_some());

    let projection = db
        .load_import_list(list_request(&db, crate::import::TriageTab::Pending, [(0, 50)]).await)
        .await
        .unwrap();
    assert_eq!(projection.summary.counts.pending, 1);
    assert_eq!(projection.summary.counts.done, 0);
    let detail = db
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .unwrap();
    assert!(!detail.is_added);
    assert_eq!(detail.imported, None);
}

/// Done is keyed by the folder's hash, not its path: a folder imported here
/// and then moved or renamed is Done where it is now.
#[tokio::test]
async fn a_moved_folder_stays_done() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let imported = scanned(&db, root, "release").await;
    record_import(&db, &imported.files.content_hash()).await;

    let moved = scanned(&db, root, "renamed").await;
    assert_eq!(moved.files.content_hash(), imported.files.content_hash());
    let projection = db
        .load_import_list(list_request(&db, crate::import::TriageTab::Done, [(0, 50)]).await)
        .await
        .unwrap();
    assert_eq!(candidate_names(&projection), vec!["renamed".to_string()]);
    assert_eq!(projection.summary.counts.pending, 0);
}

/// Deleting the release takes this device's record of the import with it, so
/// its folder is back in Found.
#[tokio::test]
async fn deleting_the_release_puts_its_folder_back_in_found() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let candidate = scanned(&db, root, "release").await;
    record_import(&db, &candidate.files.content_hash()).await;

    let mut live = db
        .subscribe_import_list(list_request(&db, crate::import::TriageTab::Done, [(0, 50)]).await);
    assert_eq!(
        live.next()
            .await
            .expect("the query stays open")
            .result
            .unwrap()
            .total_count,
        1
    );

    exec(&db, "DELETE FROM releases WHERE id = ?1", &[RELEASE_ID]).await;
    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("deleting the release wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 0, "Done no longer holds the row");
    assert_eq!(deleted.summary.counts.pending, 1);
}

/// One hash is one release on this device: a second release recorded against
/// files this device already imported is refused.
#[tokio::test]
async fn one_folder_hash_records_one_release() {
    let (db, _temp) = live_db().await;
    record_import(&db, "folder-hash").await;
    let mut copy = db.find_release_by_id(RELEASE_ID).await.unwrap().unwrap();
    copy.id = bae_test_support::test_uuid("second-imported-copy");
    db.insert_release(&copy).await.unwrap();
    let second = copy.id.clone();
    let refused = db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
                rusqlite::params![second, "folder-hash"],
            )
            .map(|_| ())
            .map_err(crate::db::DbError::from)
        })
        .await;
    assert!(
        refused.is_err(),
        "a second release of the same files is refused"
    );
}

/// The pane of a candidate its import put in the library is placed Done, which
/// carries nothing of the candidate's draft: its folder check and its draft's
/// catalogs belong to a candidate still in the queue. Its import status names
/// the release imported from it.
#[tokio::test]
async fn the_pane_of_an_imported_candidate_is_placed_done() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let candidate = scanned(&db, root, "release").await;
    let key = candidate.key();
    let detail = |db: Database, key: String| async move {
        db.load_import_candidate(&key)
            .await
            .unwrap()
            .expect("the scanned candidate has a pane")
            .resolve()
    };

    assert!(matches!(
        detail(db.clone(), key.clone()).await.placement,
        crate::import::CandidatePanePlacement::Pending { .. }
    ));

    record_import(&db, &candidate.files.content_hash()).await;
    let done = detail(db.clone(), key).await;
    assert_eq!(done.placement, crate::import::CandidatePanePlacement::Done);
    assert_eq!(
        done.import_status,
        Some(crate::import::CandidateImportStatus::Complete {
            release: crate::import::ImportedRelease {
                release_id: RELEASE_ID.to_string(),
                album_id: ALBUM_ID.to_string(),
            },
        })
    );
    assert!(!done
        .live
        .actions
        .contains(&crate::import::CandidateAction::Import));
}

/// Moving the window is a request change, not a commit: the query reruns and
/// says so without anything having been written.
#[tokio::test]
async fn import_list_moving_the_window_reruns_without_a_commit() {
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

    let request = list_request(&db, crate::import::TriageTab::Pending, [(0, 1)]).await;
    let list = request.list;
    let live = db.subscribe_import_list(request);
    let requests = live.requests();
    let mut live = live;
    let initial = live.next().await.expect("the query stays open");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert_eq!(
        candidate_names(&initial.result.unwrap()),
        vec!["first".to_string()]
    );

    requests.set(through(list, [(1, 1)])).unwrap();
    let moved = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the window change reruns the query")
        .expect("the query stays open");
    assert_eq!(moved.cause, coven::LiveQueryCause::Request);
    assert_eq!(
        candidate_names(&moved.result.unwrap()),
        vec!["second".to_string()]
    );
}

/// A commit that touches a table the list does not read does not read the list
/// again: the tab does not re-render for a write it cannot show.
#[tokio::test]
async fn import_list_withholds_a_commit_that_changes_nothing_it_reads() {
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

    let request = list_request(&db, crate::import::TriageTab::Pending, [(0, 1)]).await;
    let list = request.list;
    let mut live = db.subscribe_import_list(request);
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    // What a walk records about the directories it read: the list projects
    // candidates, and never reads this.
    exec(
        &db,
        "INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at) \
         VALUES (?1, ?1, 1234)",
        &[root.as_str()],
    )
    .await;

    let moved = next_run_is_only_the_request(&mut live, through(list, [(0, 1), (1, 1)]))
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].items, initial.windows[0].items);
}

/// The pane's candidate read moves between candidates on one subscription:
/// each value answers the key set last, and no key reads nothing.
#[tokio::test]
async fn import_candidate_moves_between_candidates_on_one_subscription() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let mut keys = Vec::new();
    for name in ["first", "second"] {
        let item = scan_candidate(root, name);
        let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
            unreachable!("the fixture builds a valid candidate");
        };
        keys.push(candidate.path.to_string_lossy().into_owned());
        db.save_folder_scan_item(root, generation, &item)
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let mut live = db.subscribe_import_candidate(None);
    let requests = live.requests();
    assert!(
        live.next()
            .await
            .expect("the query stays open")
            .result
            .unwrap()
            .is_none(),
        "no key reads nothing"
    );

    for (key, name) in keys.iter().zip(["first", "second"]) {
        requests.set(Some(key.clone())).unwrap();
        let read = live.next().await.expect("the query stays open");
        assert_eq!(read.request.as_deref(), Some(key.as_str()));
        let detail = read.result.unwrap().expect("the candidate reads");
        assert_eq!(detail.candidate.name, name);
    }

    requests.set(None).unwrap();
    assert!(live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .is_none());
}
