//! One read of the import list against a records, and its live query: what
//! reruns it, what it withholds, how Done follows what this device imported,
//! and a window read from the last read's rows only while none has moved.

use super::*;
use crate::selection::SelectionChange;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::{live_db, RELEASE_ID};
use bae_store::test_fixtures::empty_db;
use bae_store::test_fixtures::live_query::next_run_is_only_the_request;
use bae_store::test_fixtures::window_cost::{offsets, report, LIMIT, RUNS};
use bae_store::{LibraryPageWindow, LibraryPageWindows};
use bae_util::test_channels::HANG_GUARD;
use std::time::Duration;
use std::time::Instant;

fn candidate_names(projection: &crate::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_this_device_imports_its_files() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Pending, [(0, 50)]).await);
    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.total_count, 1);
    let initial_counts = tables.load_tab_counts().await.unwrap();
    assert_eq!((initial_counts.pending, initial_counts.done), (1, 0));

    imported_release(&db, &candidate).await;

    let imported = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("the recorded import wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(imported.total_count, 0, "Pending no longer holds the row");
    assert_eq!(tables.load_tab_counts().await.unwrap().done, 1);
}

/// A release in the library that this device did not import — one another
/// device imported from the same files and synced here — leaves the folder in
/// Found like any other.
#[tokio::test]
async fn a_release_this_device_did_not_import_leaves_the_folder_in_found() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;
    assert!(records.find_release_by_id(RELEASE_ID).await.unwrap().is_some());

    let counts = tables.load_tab_counts().await.unwrap();
    assert_eq!((counts.pending, counts.done), (1, 0));
    let detail = tables.load_import_candidate(&candidate.key()).await.unwrap().unwrap();
    assert!(detail.draft().is_some(), "the folder holds a draft, as one nobody imported does");
}

/// Done is keyed by the folder's hash, not its path: a folder imported here
/// and then moved or renamed is Done where it is now.
#[tokio::test]
async fn a_moved_folder_stays_done() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let imported = scan_finished(&db, root, "release").await;
    imported_release(&db, &imported).await;

    let moved = scan_finished(&db, root, "renamed").await;
    assert_eq!(moved.files.content_hash(), imported.files.content_hash());
    let projection = tables
        .load_import_list(list_request(&db, crate::TriageTab::Done, [(0, 50)]).await)
        .await
        .unwrap();
    assert_eq!(candidate_names(&projection), vec!["renamed".to_string()]);
    assert_eq!(tables.load_tab_counts().await.unwrap().pending, 0);
}

/// Deleting the release takes this device's record of the import with it, so
/// its folder is back in Found.
#[tokio::test]
async fn deleting_the_release_puts_its_folder_back_in_found() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;
    let release = imported_release(&db, &candidate).await;

    let mut live =
        tables.subscribe_import_list(list_request(&db, crate::TriageTab::Done, [(0, 50)]).await);
    assert_eq!(live.next().await.expect("the query stays open").result.unwrap().total_count, 1);

    bae_records::test_fixtures::delete_release(
        &bae_records::test_fixtures::test_records(&db),
        &release.id,
    )
    .await;
    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("deleting the release wakes the list")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 0, "Done no longer holds the row");
    assert_eq!(tables.load_tab_counts().await.unwrap().pending, 1);
}

/// Moving the window is a request change, not a commit: the query reruns and
/// says so without anything having been written.
#[tokio::test]
async fn import_list_moving_the_window_reruns_without_a_commit() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
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
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
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
    tables.record_folder_scan_directories(root, &[(root.to_path_buf(), 1234)]).await.unwrap();

    let moved = next_run_is_only_the_request(&mut live, through(list, [(0, 1), (1, 1)]))
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].rows, initial.windows[0].rows);
}

/// The list is a live query over the scan tables, so a scan item written while
/// someone is watching wakes it. A candidate's rows span several tables now; a
/// dependency missed on any of them would leave the list showing the previous
/// scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
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
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
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
    assert_eq!(moved.windows[0].rows, initial.windows[0].rows);
}

/// A scan's progress moves the folder scans' own query and sends the list
/// subscription nothing: the next list value after it is the one a new
/// request asks for, with the rows its last read held.
#[tokio::test]
async fn a_scans_progress_moves_the_scans_and_sends_the_list_nothing() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
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
    let mut scans = tables.subscribe_scan_found_counts();
    let initial = subscription.next().await.expect("the query stays open");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
    let found = |scans: crate::ScanFoundCounts| match scans.into_iter().collect::<Vec<_>>()[..] {
        [(_, found_count)] => found_count,
        ref scans => panic!("the rescan is in flight: {scans:?}"),
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
    assert_eq!(next.result.unwrap().windows[0].rows, initial.result.unwrap().windows[0].rows);
}

/// The import list's items in the windows `request` names, as one read with
/// no earlier read's rows to start from finds them.
async fn import_items(
    db: &Store,
    request: crate::ImportListRequest,
) -> Vec<Vec<crate::list::ImportListItem>> {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .load_import_list(request)
        .await
        .unwrap()
        .windows
        .into_iter()
        .map(|window| window.rows)
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
                    .map(|window| window.rows)
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

/// A text-filtered import list scrolled a window at a time to its end and
/// back holds at each window what a read by that window's offset holds: a
/// window beside a row the list placed is read in the list's order, and a
/// jump reads the rows the search finds.
#[tokio::test(flavor = "multi_thread")]
async fn a_scrolled_filtered_import_list_holds_what_a_jump_reads() {
    use crate::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    grow_import_list(&db, tmp.path(), 0, 2_000).await;
    let list = tables
        .open_import_list(ImportListView {
            tab: crate::TriageTab::Pending,
            filter_text: "Album 1".to_string(),
            pending_filter: PendingFilter::All,
            order: ImportListOrder::PathAscending,
        })
        .await
        .unwrap();
    let request = |offset| ImportListRequest {
        list,
        windows: LibraryPageWindows::from([LibraryPageWindow { offset, limit: 10 }]),
    };
    let total = tables.load_import_list(request(0)).await.unwrap().total_count;
    assert!(total > 100, "the filter keeps enough rows to scroll through, kept {total}");
    let mut query = tables.subscribe_import_list(request(0));
    query.next().await.unwrap().result.unwrap();
    let requests = query.requests();
    let mut scroll = async |offset: u64| {
        requests.set(request(offset)).unwrap();
        loop {
            let event = query.next().await.unwrap();
            if event.request == request(offset) {
                return event.result.unwrap().windows.remove(0).rows;
            }
        }
    };
    let last = total - total % 10;
    let down = (1..=last / 10).map(|window| window * 10);
    let up = (0..last / 10).rev().map(|window| window * 10);
    for offset in down.chain(up) {
        assert_eq!(
            scroll(offset).await,
            import_items(&db, request(offset)).await.remove(0),
            "at {offset}"
        );
    }
}

/// A text-filtered import list's live query, which keeps what its search
/// counted while the list stands as it stood, counts again when a scan adds
/// matching folders, when its groups fold and when they open, as a read with
/// nothing kept does.
#[tokio::test(flavor = "multi_thread")]
async fn a_filtered_import_lists_counts_follow_the_scan_and_the_folds() {
    use crate::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    grow_grouped_import_list(&db, tmp.path(), 0, 100).await;
    let list = tables
        .open_import_list(ImportListView {
            tab: crate::TriageTab::Pending,
            filter_text: "Album 1".to_string(),
            pending_filter: PendingFilter::All,
            order: ImportListOrder::PathAscending,
        })
        .await
        .unwrap();
    let request = ImportListRequest {
        list,
        windows: LibraryPageWindows::from([LibraryPageWindow { offset: 0, limit: 10 }]),
    };
    let mut query = tables.subscribe_import_list(request.clone());
    let counts =
        async |read: crate::ImportListProjection| (read.total_count, read.summary.narrowed);
    let before = counts(query.next().await.unwrap().result.unwrap()).await;
    assert_eq!(before, counts(tables.load_import_list(request.clone()).await.unwrap()).await);
    let mut follows = async |what: &str| {
        let fresh = counts(tables.load_import_list(request.clone()).await.unwrap()).await;
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                let read = counts(query.next().await.unwrap().result.unwrap()).await;
                if read == fresh {
                    return read;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the live counts reach {fresh:?} after {what}"))
    };
    tables.set_import_list_groups_folded(list, true).await.unwrap();
    let folded = follows("folding every group").await;
    assert!(folded.0 < before.0, "folded groups hide their members: {folded:?} of {before:?}");
    tables.set_import_list_groups_folded(list, false).await.unwrap();
    assert_eq!(follows("opening every group").await, before);
    grow_grouped_import_list(&db, tmp.path(), 100, 200).await;
    let grown = follows("a scan adds matching folders").await;
    assert!(grown.0 > before.0, "the scan's folders count: {grown:?} of {before:?}");
}

impl ImportTables {
    /// Where the header of each item of `list`'s window at `window` sits,
    /// read as a read with no earlier read's places reads it, and each
    /// statement that read of the header positions ran, with its plan.
    async fn import_list_header_positions(
        &self,
        list: ImportListId,
        window: LibraryPageWindow,
    ) -> (Vec<Option<u64>>, Vec<(String, Vec<String>)>) {
        self.store
            .read(move |sql| {
                let row = list_row(&sql, list)?.expect("the test's list is open");
                let listed = summary::listed(&sql, &row, &summary::counts(&sql)?, None)?;
                let items = row.items(listed.shown(&window, &Places::new()));
                let (keys, read) = items.window(&sql, &window, &Places::new(), listed.reach())?;
                items.take_ran();
                let positions = items.header_positions(
                    &sql,
                    window.offset,
                    &keys,
                    read.get(&window.offset),
                    &mut HeaderPlaces::new(),
                )?;
                let ran = items
                    .take_ran()
                    .into_iter()
                    .map(|statement| {
                        // The plan is the same whatever the parameters hold.
                        let mut names: Vec<String> = statement
                            .split(':')
                            .skip(1)
                            .map(|after| {
                                let name: String = after
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                                    .collect();
                                format!(":{name}")
                            })
                            .collect();
                        names.sort();
                        names.dedup();
                        let unbound: Option<i64> = None;
                        let params: Vec<(&str, &dyn rusqlite::ToSql)> =
                            names.iter().map(|name| (name.as_str(), &unbound as _)).collect();
                        let plan = sql.query(
                            &format!("EXPLAIN QUERY PLAN {statement}"),
                            params.as_slice(),
                            |row| row.get::<_, String>(3),
                        )?;
                        Ok((statement, plan))
                    })
                    .collect::<Result<_, DbError>>()?;
                Ok((positions, ran))
            })
            .await
            .unwrap()
    }
}

/// A list's window that starts with members of a group whose header is above
/// it finds the header's place by counting the members of the group before
/// the window, off the indexes of the group alone, not by numbering the
/// list's items from the top: a search's list, read from the rows its search
/// finds or from its tab's rows, alike; and a write that makes a row in the
/// middle of the list a match moves the places it counts.
#[tokio::test]
async fn a_list_counts_a_members_group_to_its_header() {
    use crate::{ImportListOrder, ImportListView, PendingFilter, TriageTab};
    let lib = Library::new().await;
    let folders: Vec<String> = (0..6)
        .flat_map(|g| {
            [format!("Group {g}/Wanted {g}a"), format!("Group {g}/Wanted {g}b")]
                .into_iter()
                .chain((0..8).map(move |o| format!("Group {g}/Other {g}{o}")))
        })
        .collect();
    let folders: Vec<&str> = folders.iter().map(String::as_str).collect();
    lib.run(scanned_with_states(&lib, &folders)).await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    // Where each item's header sits in a read of the whole list.
    let headers_in = |items: &[String]| -> Vec<Option<u64>> {
        let mut header = None;
        items
            .iter()
            .enumerate()
            .map(|(at, item)| match item.starts_with("group ") {
                true => {
                    header = Some(at as u64);
                    None
                }
                false => header,
            })
            .collect()
    };
    for filter_text in ["wanted", ""] {
        let list = tables
            .open_import_list(ImportListView {
                tab: TriageTab::Pending,
                filter_text: filter_text.to_string(),
                pending_filter: PendingFilter::All,
                order: ImportListOrder::PathAscending,
            })
            .await
            .unwrap();
        for (write, first_member) in [
            (Write::nothing(), "candidate Group 2/Wanted 2b"),
            (lib.draft("Group 3/Other 30", "Wanted Now"), "candidate Group 3/Wanted 3a"),
        ] {
            lib.run(write).await;
            let items = sequence(&lib.read(list).await);
            let offset = items.iter().position(|item| item == first_member).unwrap();
            let window = LibraryPageWindow { offset: offset as u64, limit: 3 };
            let (positions, ran) = tables.import_list_header_positions(list, window).await;
            assert_eq!(positions, headers_in(&items)[offset..offset + 3], "{items:?}");
            assert!(!ran.is_empty(), "the header above the window is found by a read");
            for (statement, plan) in ran {
                assert!(!statement.contains(" OVER "), "numbers the items: {statement}");
                assert!(
                    !plan.iter().any(|step| step.starts_with("SCAN") && !step.contains("subquery")),
                    "reads a table whole: {statement}: {plan:?}"
                );
            }
        }
        tables.close_import_list(list).await.unwrap();
    }
}

/// Scan `start..end` more folders into the import list, each its own album
/// folder under one of 100 artist folders, in a watched folder of their own.
pub(crate) async fn grow_import_list(
    db: &Store,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    scan_folders(db, tmp, start, end, |i| {
        format!("Artist {:03}/Album {:06}", (i * 7) % 100, (i * 16807) % 999_983)
    })
    .await
}

/// Scan `start..end` more folders as `Group {g:05}/Album {i}`, five albums to
/// a group, so every five albums have a header, all of them open.
async fn grow_grouped_import_list(
    db: &Store,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    scan_folders(db, tmp, start, end, |i| format!("Group {:05}/Album {i}", i / 5)).await
}

/// Scan `start..end` more folders as `Album {i}`, none below another folder,
/// so the list holds no group.
async fn grow_flat_import_list(
    db: &Store,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    scan_folders(db, tmp, start, end, |i| format!("Album {i}")).await
}

/// Scan the folders `name(start)..name(end)` into a watched folder of their own.
async fn scan_folders(
    db: &Store,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
    name: impl Fn(u64) -> String,
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
    // In batches, as the scan writes what it reads.
    let ends: Vec<u64> = (start..end).step_by(1_000).map(|at| (at + 1_000).min(end)).collect();
    for (from, to) in (start..end).step_by(1_000).zip(ends) {
        let items = (from..to)
            .map(|i| {
                crate::tables::folder_scans::FolderEntryToWrite::Release(
                    crate::tables::folder_scans::ScanItemToWrite {
                        item: crate::folder_scanner::ScanItem::Valid(candidate(
                            std::path::Path::new(&root),
                            &name(i),
                        )),
                        seed: crate::tables::folder_scans::ScanItemSeed::Tags(None),
                        folder_date: None,
                    },
                )
            })
            .collect();
        tables
            .save_folder_scan_items(std::path::Path::new(&root), generation, items)
            .await
            .unwrap()
            .expect("the open scan takes the entries");
    }
    tables
        .finish_folder_scan(std::path::Path::new(&root), generation, None)
        .await
        .unwrap()
        .unwrap();
    started.elapsed()
}

#[path = "read_tests/cost.rs"]
mod cost;

#[path = "read_tests/plain_cost.rs"]
mod plain_cost;
