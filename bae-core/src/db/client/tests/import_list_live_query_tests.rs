//! The import list's live query: what reruns it, what it withholds, and what
//! a Done row reads from the library.

use super::super::*;
use super::exec;
use super::live_query_tests::{
    live_db, next_run_is_only_the_request, ALBUM_ID, IDENTITY_ID, RELEASE_ID,
};
use std::time::Duration;
/// The import list's request carries the view and the windows, so every test
/// below states both.
pub(super) fn list_request(
    tab: crate::import::TriageTab,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::import::ImportListRequest {
    crate::import::ImportListRequest {
        view: crate::import::ImportListView {
            tab,
            order: crate::import::ImportListOrder::PathAscending,
            ..crate::import::ImportListView::default()
        },
        windows: windows
            .into_iter()
            .map(|(offset, limit)| crate::library::LibraryPageWindow { offset, limit })
            .collect(),
        upload_standing: Default::default(),
        live_standings: Default::default(),
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

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_its_content_hash_is_imported() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live.next().await.result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.summary.counts.pending, 1);
    assert_eq!(initial.summary.counts.done, 0);

    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[content_hash.as_str(), RELEASE_ID],
    )
    .await;

    let imported = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the imported release wakes the list")
        .result
        .unwrap();
    assert_eq!(imported.total_count, 0, "Pending no longer holds the row");
    assert_eq!(imported.summary.counts.done, 1);
}

/// The Done rows one read of the list holds.
fn imported_rows(
    projection: &crate::import::ImportListProjection,
) -> Vec<crate::import::ImportedRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Imported { row, .. } => Some(row.clone()),
            _ => None,
        })
        .collect()
}

/// A Done row is the library release its import became, and every part of it
/// is read from the library inside the list's own query: a new cover, a
/// catalog record and the release's deletion each rerun the list and reach the
/// row.
#[tokio::test]
async fn import_list_done_row_reads_the_library_release() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[content_hash.as_str(), RELEASE_ID],
    )
    .await;

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Done, [(0, 50)]));
    let initial = imported_rows(&live.next().await.result.unwrap());
    assert_eq!(initial.len(), 1);
    assert!(
        initial[0].probably_in_library,
        "the release carries the folder's hash, but no import from the folder wrote it"
    );
    let release = &initial[0].releases[0];
    assert_eq!(release.release_id, RELEASE_ID);
    assert_eq!(release.album_id, ALBUM_ID);
    assert_eq!(release.title, "Album Title");
    assert_eq!(release.artist.as_deref(), Some("Artist Name"));
    assert!(release.cover.is_none());
    assert!(release.records.is_empty());

    let cover_blob = "bd5c1f6c-3b6e-4d16-9f0a-2c1d5f61a0aa";
    super::write_cover(&db, RELEASE_ID, cover_blob).await;
    let covered = imported_rows(
        &tokio::time::timeout(Duration::from_secs(2), live.next())
            .await
            .expect("a new cover wakes the list")
            .result
            .unwrap(),
    );
    assert_eq!(
        covered[0].releases[0].cover,
        Some(crate::album_detail::ImageRef {
            id: RELEASE_ID.to_string(),
            version: cover_blob.to_string(),
            image_type: crate::db::LibraryImageType::Cover,
        })
    );

    exec(
        &db,
        "INSERT INTO release_records
         (id, release_id, catalog, kind, key, album_key, url, created_at) VALUES (?1, ?2, 'musicbrainz', 'pressing', 'source-release-1', 'source-group-1', 'https://musicbrainz.org/release/source-release-1', '2026-01-01T00:00:00Z')",
        &[IDENTITY_ID, RELEASE_ID],
    )
    .await;
    let recorded = imported_rows(
        &tokio::time::timeout(Duration::from_secs(2), live.next())
            .await
            .expect("a catalog record wakes the list")
            .result
            .unwrap(),
    );
    assert_eq!(
        recorded[0].releases[0]
            .records
            .iter()
            .map(|record| record.catalog())
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz]
    );

    exec(&db, "DELETE FROM releases WHERE id = ?1", &[RELEASE_ID]).await;
    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("deleting the release wakes the list")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 0, "Done no longer holds the row");
    assert_eq!(deleted.summary.counts.pending, 1);
}

/// The filter finds a Done row by the library title it shows, read in the
/// list's own query: renaming the album reruns a filtered list that holds no
/// window at all, and the row is then found by its new title and not its old.
#[tokio::test]
async fn import_list_filter_finds_a_done_row_by_its_library_title() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[content_hash.as_str(), RELEASE_ID],
    )
    .await;

    let filtered = |filter: &str| {
        let mut request = list_request(crate::import::TriageTab::Done, []);
        request.view.filter_text = filter.to_string();
        request
    };
    let live = db.subscribe_import_list(filtered("album title"));
    let requests = live.requests();
    let mut live = live;
    assert_eq!(live.next().await.result.unwrap().total_count, 1);

    exec(
        &db,
        "UPDATE albums SET title = 'Album' WHERE id = ?1",
        &[ALBUM_ID],
    )
    .await;
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("renaming the album wakes a filtered list")
        .result
        .unwrap();
    assert_eq!(renamed.total_count, 0, "the old title is no longer shown");

    requests.set(filtered("ALBUM")).unwrap();
    let found = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the filter change reruns the query")
        .result
        .unwrap();
    assert_eq!(found.total_count, 1, "the new title finds the row");
}

/// The pane of a candidate its import put in the library is placed Done, which
/// carries nothing of the candidate's draft: its folder check and its draft's
/// catalogs belong to a candidate still in the queue.
#[tokio::test]
async fn the_pane_of_an_imported_candidate_is_placed_done() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let key = candidate.path.to_string_lossy().into_owned();
    let content_hash = candidate.files.content_hash();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let placement = |db: Database, key: String| async move {
        db.load_import_candidate(&key)
            .await
            .unwrap()
            .expect("the scanned candidate has a pane")
            .resolve(&crate::import::TriageRuntimeFacts::default())
            .placement
    };

    assert!(matches!(
        placement(db.clone(), key.clone()).await,
        crate::import::CandidatePanePlacement::Pending { .. }
    ));

    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[content_hash.as_str(), RELEASE_ID],
    )
    .await;
    assert_eq!(
        placement(db.clone(), key).await,
        crate::import::CandidatePanePlacement::Done
    );
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

    let live = db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 1)]));
    let requests = live.requests();
    let mut live = live;
    let initial = live.next().await;
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert_eq!(
        candidate_names(&initial.result.unwrap()),
        vec!["first".to_string()]
    );

    requests
        .set(list_request(crate::import::TriageTab::Pending, [(1, 1)]))
        .unwrap();
    let moved = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the window change reruns the query");
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

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 1)]));
    let initial = live.next().await.result.unwrap();
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

    let moved = next_run_is_only_the_request(
        &mut live,
        list_request(crate::import::TriageTab::Pending, [(0, 1), (1, 1)]),
    )
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
        live.next().await.result.unwrap().is_none(),
        "no key reads nothing"
    );

    for (key, name) in keys.iter().zip(["first", "second"]) {
        requests.set(Some(key.clone())).unwrap();
        let read = live.next().await;
        assert_eq!(read.request.as_deref(), Some(key.as_str()));
        let detail = read.result.unwrap().expect("the candidate reads");
        assert_eq!(detail.candidate.name, name);
    }

    requests.set(None).unwrap();
    assert!(live.next().await.result.unwrap().is_none());
}

#[tokio::test]
async fn done_lists_every_copy_of_a_folder() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::watched_folder::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db
        .begin_folder_scan(root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    let mut copy = db.find_release_by_id(RELEASE_ID).await.unwrap().unwrap();
    copy.content_hash = Some(candidate.files.content_hash());
    exec(
        &db,
        "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
        &[copy.content_hash.as_deref().unwrap(), RELEASE_ID],
    )
    .await;
    copy.id = bae_test_support::test_uuid("second-imported-copy");
    let mut other_album = db.find_album_by_id(ALBUM_ID).await.unwrap().unwrap();
    other_album.id = bae_test_support::test_uuid("second-copy-album");
    other_album.title = "Second Album".to_string();
    other_album.primary_release_id = None;
    db.insert_album(&other_album).await.unwrap();
    copy.album_id = other_album.id;
    db.insert_release(&copy).await.unwrap();

    let projection = db
        .load_import_list(list_request(crate::import::TriageTab::Done, [(0, 50)]))
        .await
        .expect("every release with the folder hash is listed");
    assert_eq!(projection.summary.counts.done, 1, "Done counts folders");
    let rows = imported_rows(&projection);
    assert_eq!(rows.len(), 1);
    let mut ids: Vec<_> = rows[0]
        .releases
        .iter()
        .map(|release| release.release_id.clone())
        .collect();
    ids.sort();
    let mut expected = vec![RELEASE_ID.to_string(), copy.id.clone()];
    expected.sort();
    assert_eq!(ids, expected);

    let detail = db
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .unwrap();
    assert!(
        detail.is_added,
        "the matching hash supplies the duplicate hint"
    );
    assert_eq!(detail.imported_releases.len(), 2);

    let detail = detail.resolve(&crate::import::TriageRuntimeFacts::default());
    assert_eq!(
        detail.placement,
        crate::import::CandidatePanePlacement::Done
    );
    assert!(!detail
        .live
        .actions
        .contains(&crate::import::CandidateAction::Import));

    for title in ["Album Title", "Second Album"] {
        let mut request = list_request(crate::import::TriageTab::Done, [(0, 50)]);
        request.view.filter_text = title.to_string();
        let filtered = db.load_import_list(request).await.unwrap();
        assert_eq!(filtered.total_count, 1, "search finds either copy");
        assert_eq!(imported_rows(&filtered)[0].releases.len(), 2);
    }

    exec(&db, "DELETE FROM releases WHERE id = ?1", &[RELEASE_ID]).await;
    let surviving = db
        .load_import_list(list_request(crate::import::TriageTab::Done, [(0, 50)]))
        .await
        .unwrap();
    assert_eq!(surviving.summary.counts.done, 1);
    let surviving = imported_rows(&surviving);
    assert_eq!(surviving[0].releases.len(), 1);
    assert_eq!(surviving[0].releases[0].release_id, copy.id);
}
