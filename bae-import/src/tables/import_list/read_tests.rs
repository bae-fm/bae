//! One read of the import list against a database, and its live query: what
//! reruns it, what it withholds, how Done follows what this device imported,
//! and a window read from the last read's rows only while none has moved.

use super::*;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use bae_db::test_fixtures::live_query::{live_db, next_run_is_only_the_request, RELEASE_ID};
use bae_db::test_fixtures::window_cost::{offsets, report, LIMIT, RUNS, SIZES};
use bae_db::test_fixtures::{empty_db, exec};
use bae_db::{LibraryPageWindow, LibraryPageWindows};
use bae_util::test_channels::HANG_GUARD;
use std::time::Duration;
use std::time::Instant;

fn candidate_names(projection: &crate::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_this_device_imports_its_files() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.summary.counts.pending, 1);
    assert_eq!(initial.summary.counts.done, 0);

    record_import(&db, &candidate.files.content_hash()).await;

    let imported = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("the recorded import wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(imported.total_count, 0, "Pending no longer holds the row");
    assert_eq!(imported.summary.counts.done, 1);
}

/// The list's summary names the watched folders as every read of them does:
/// stored rows that overlap, which the add refuses, are corrupt and fail the
/// read rather than being listed.
#[tokio::test]
async fn overlapping_stored_roots_fail_the_list_read() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let outer = crate::tables::watched_path::host_root("/music");
    let inner = crate::tables::watched_path::host_root("/music/artist");
    tables.add_watched_import_folder(&outer).await.unwrap();
    let request = list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await;
    tables.load_import_list(request.clone()).await.expect("one watched folder reads");
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO watched_import_folders (path, position) VALUES (?, 1)",
            [coven::path_bytes(&inner)],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let error = tables.load_import_list(request).await.unwrap_err();
    assert!(error.to_string().contains("cannot overlap"), "{error}");
}

/// A release in the library that this device did not import — one another
/// device imported from the same files and synced here — leaves the folder in
/// Found like any other.
#[tokio::test]
async fn a_release_this_device_did_not_import_leaves_the_folder_in_found() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;
    assert!(db.find_release_by_id(RELEASE_ID).await.unwrap().is_some());

    let projection = tables
        .load_import_list(list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await)
        .await
        .unwrap();
    assert_eq!(projection.summary.counts.pending, 1);
    assert_eq!(projection.summary.counts.done, 0);
    let detail = tables.load_import_candidate(&candidate.key()).await.unwrap().unwrap();
    assert!(detail.draft().is_some(), "the folder holds a draft, as one nobody imported does");
}

/// Done is keyed by the folder's hash, not its path: a folder imported here
/// and then moved or renamed is Done where it is now.
#[tokio::test]
async fn a_moved_folder_stays_done() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let imported = scan_finished(&db, root, "release").await;
    record_import(&db, &imported.files.content_hash()).await;

    let moved = scan_finished(&db, root, "renamed").await;
    assert_eq!(moved.files.content_hash(), imported.files.content_hash());
    let projection = tables
        .load_import_list(list_request(&db, crate::TriageTab::Done, [(0, 50)]).await)
        .await
        .unwrap();
    assert_eq!(candidate_names(&projection), vec!["renamed".to_string()]);
    assert_eq!(projection.summary.counts.pending, 0);
}

/// Deleting the release takes this device's record of the import with it, so
/// its folder is back in Found.
#[tokio::test]
async fn deleting_the_release_puts_its_folder_back_in_found() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;
    record_import(&db, &candidate.files.content_hash()).await;

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Done, [(0, 50)]).await);
    assert_eq!(live.next().await.expect("the query stays open").result.unwrap().total_count, 1);

    exec(&db, "DELETE FROM releases WHERE id = ?1", &[RELEASE_ID]).await;
    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("deleting the release wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 0, "Done no longer holds the row");
    assert_eq!(deleted.summary.counts.pending, 1);
}

/// Moving the window is a request change, not a commit: the query reruns and
/// says so without anything having been written.
#[tokio::test]
async fn import_list_moving_the_window_reruns_without_a_commit() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, generation, &scan_candidate(root, name)).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let request = list_request(&db, crate::TriageTab::Pending, [(0, 1)]).await;
    let list = request.list;
    let live = tables.subscribe_import_list(request);
    let requests = live.requests();
    let mut live = live;
    let initial = live.next().await.expect("the query stays open");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    assert_eq!(candidate_names(&initial.result.unwrap()), vec!["first".to_string()]);

    requests.set(through(list, [(1, 1)])).unwrap();
    let moved = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("the window change reruns the query")
        .expect("the query stays open");
    assert_eq!(moved.cause, coven::LiveQueryCause::Request);
    assert_eq!(candidate_names(&moved.result.unwrap()), vec!["second".to_string()]);
}

/// A commit that touches a table the list does not read does not read the list
/// again: the tab does not re-render for a write it cannot show.
#[tokio::test]
async fn import_list_withholds_a_commit_that_changes_nothing_it_reads() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, generation, &scan_candidate(root, name)).await.unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let request = list_request(&db, crate::TriageTab::Pending, [(0, 1)]).await;
    let list = request.list;
    let mut live = tables.subscribe_import_list(request);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    // What a walk records about the directories it read: the list projects
    // candidates, and never reads this.
    let root_bytes = hex::encode(coven::path_bytes(root));
    exec(
        &db,
        &format!(
            "INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at) \
             VALUES (X'{root_bytes}', X'{root_bytes}', 1234)"
        ),
        &[],
    )
    .await;

    let moved = next_run_is_only_the_request(&mut live, through(list, [(0, 1), (1, 1)]))
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].items, initial.windows[0].items);
}

/// The list is a live query over the scan tables, so a scan item written while
/// someone is watching wakes it. A candidate's rows span several tables now; a
/// dependency missed on any of them would leave the list showing the previous
/// scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = tokio::time::timeout(HANG_GUARD, live.next())
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
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
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
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
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
    let progressed = tokio::time::timeout(HANG_GUARD, scans.next())
        .await
        .expect("the re-confirmed folder moves the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(found(progressed), 1);

    requests.set(through(list, [(0, 49)])).unwrap();
    let next = tokio::time::timeout(HANG_GUARD, subscription.next())
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

/// The import list's items in the windows `request` names, as one read with
/// no earlier read's rows to start from finds them.
async fn import_items(
    db: &Database,
    request: crate::ImportListRequest,
) -> Vec<Vec<crate::list::ImportListItem>> {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .load_import_list(request)
        .await
        .unwrap()
        .windows
        .into_iter()
        .map(|window| window.items)
        .collect()
}

/// A live import list of grouped folders, scrolled a window at a time down
/// and back up, holds at each window what a read with nothing to start from
/// holds, its members' headers included; and so it does after a scan adds
/// folders above the window it scrolls from.
#[tokio::test(flavor = "multi_thread")]
async fn a_scrolled_import_list_holds_its_offsets_items_across_a_scan() {
    use crate::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    grow_import_list(&db, tmp.path(), 0, 300).await;
    let list = tables
        .open_import_list(ImportListView {
            tab: crate::TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
            // The folders a later scan adds sort first.
            order: ImportListOrder::PathDescending,
        })
        .await
        .unwrap();
    let request = |offset| ImportListRequest {
        list,
        windows: LibraryPageWindows::from([LibraryPageWindow { offset, limit: 10 }]),
    };
    let mut query = tables.subscribe_import_list(request(0));
    query.next().await.unwrap().result.unwrap();
    let requests = query.requests();
    let mut scroll = async |offset: u64| {
        requests.set(request(offset)).unwrap();
        loop {
            let event = query.next().await.unwrap();
            if event.request == request(offset) {
                return event
                    .result
                    .unwrap()
                    .windows
                    .into_iter()
                    .map(|window| window.items)
                    .collect::<Vec<_>>();
            }
        }
    };
    for offset in [10, 20, 30, 40, 30, 20, 10] {
        assert_eq!(scroll(offset).await, import_items(&db, request(offset)).await, "at {offset}");
    }
    grow_import_list(&db, tmp.path(), 300, 305).await;
    for offset in [30, 20] {
        assert_eq!(
            scroll(offset).await,
            import_items(&db, request(offset)).await,
            "at {offset} after the scan"
        );
    }
}

/// Scan `start..end` more folders into the import list, each its own album
/// folder under one of 100 artist folders, in a watched folder of their own.
pub(crate) async fn grow_import_list(
    db: &Database,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = tmp.join(format!("watched-{start}"));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(&root).unwrap().to_str().unwrap().to_string();
    let started = Instant::now();
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::VolumeKind::Local)
        .await
        .unwrap();
    for i in start..end {
        let name = format!("Artist {:03}/Album {:06}", (i * 7) % 100, (i * 16807) % 999_983);
        let candidate = candidate(std::path::Path::new(&root), &name);
        tables
            .save_folder_scan_item(
                std::path::Path::new(&root),
                generation,
                &crate::folder_scanner::ScanItem::Valid(candidate),
            )
            .await
            .unwrap()
            .unwrap();
    }
    tables
        .finish_folder_scan(std::path::Path::new(&root), generation, None)
        .await
        .unwrap()
        .unwrap();
    started.elapsed()
}

/// One read of the import list's Found tab with one window, at each measured
/// offset, in each order, through the list's live query: the counts, the
/// window's items and their rows. A jump moves the window between the offset
/// and one far from it, so no read knows a row beside its window; a scroll
/// moves it between the offset and the window after it.
async fn measure_import_list(db: &Database, n: u64) {
    let tables = crate::tables::ImportTables::new(db.clone());
    use crate::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    for order in [ImportListOrder::PathAscending, ImportListOrder::NewestFirst] {
        let list = tables
            .open_import_list(ImportListView {
                tab: crate::TriageTab::Pending,
                filter_text: String::new(),
                pending_filter: PendingFilter::All,
                order,
            })
            .await
            .unwrap();
        let request = |offset| ImportListRequest {
            list,
            windows: bae_db::LibraryPageWindows::from([bae_db::LibraryPageWindow {
                offset,
                limit: LIMIT,
            }]),
        };
        for (at, offset) in offsets(n) {
            let away = if offset >= n / 2 { offset - n / 4 } else { offset + n / 4 };
            let next = if offset + 2 * LIMIT <= n { offset + LIMIT } else { offset - LIMIT };
            for (reach, other) in [("jump", away), ("scroll", next)] {
                let mut query = tables.subscribe_import_list(request(other));
                query.next().await.unwrap().result.unwrap();
                let requests = query.requests();
                let mut runs = Vec::with_capacity(RUNS);
                let mut rows = 0;
                for run in 0..2 * RUNS {
                    let target = if run % 2 == 0 { offset } else { other };
                    let started = Instant::now();
                    requests.set(request(target)).unwrap();
                    let read = query.next().await.unwrap().result.unwrap();
                    if target == offset {
                        runs.push(started.elapsed());
                        rows = read.windows[0].items.len();
                    }
                }
                runs.sort();
                report(
                    "import_list",
                    &format!("{order:?}"),
                    reach,
                    at,
                    offset,
                    n,
                    (runs[RUNS / 2], rows),
                );
            }
        }
        tables.close_import_list(list).await.unwrap();
    }
}

/// What one window of the import list costs to read, at the top, the middle
/// and the end of the list, at the sizes db's list windows are measured at,
/// reached by a jump and by scrolling through the list's live query. A
/// measurement, not a check: run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     import_list_window_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_window_cost() {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let mut held = 0;
    for n in SIZES {
        let took = grow_import_list(&db, tmp.path(), held, n).await;
        eprintln!("WINDOW_COST scanned n={n} in {:.1} s", took.as_secs_f64());
        held = n;
        measure_import_list(&db, n).await;
    }
}
