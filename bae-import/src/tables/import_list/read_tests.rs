//! The reads of the import list against a records, each its own live query:
//! what reruns each, what it withholds, how Done follows what this device
//! imported, and that the windows of a chain hold every row exactly once.

use super::*;
use crate::selection::SelectionChange;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use bae_records::test_fixtures::live_query::{live_db, RELEASE_ID};
use bae_store::list_windows::OrderKey;
use bae_store::test_fixtures::empty_db;
use bae_store::test_fixtures::window_cost::{offsets, report, LIMIT, RUNS};
use bae_store::ListWindow;
use bae_util::test_channels::HANG_GUARD;
use std::time::Duration;
use std::time::Instant;

fn candidate_names(items: &[crate::ImportListItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

/// The first `limit` items of the list, with no end.
fn first(limit: u64) -> ListWindow {
    ListWindow::Forward { from: None, until: None, limit }
}

/// Everything after `from`, with no end.
fn from(key: &OrderKey, limit: u64) -> ListWindow {
    ListWindow::Forward { from: Some(key.clone()), until: None, limit }
}

/// The window of every item before `key`.
fn until(key: &OrderKey) -> ListWindow {
    ListWindow::Forward { from: None, until: Some(key.clone()), limit: u64::MAX }
}

/// The next value of `live` that `accept` admits.
async fn next_until<T, Request: Clone + PartialEq + Send + Sync + 'static>(
    live: &mut coven::ReconfigurableLiveQuery<Request, T, DbError>,
    what: &str,
    accept: impl Fn(&T) -> bool,
) -> T
where
    T: Clone + PartialEq + Send + 'static,
{
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = live.next().await.expect("the query stays open").result.unwrap();
            if accept(&value) {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the list reaches {what}"))
}

/// The next count of `live` that `accept` admits.
async fn count_until(
    live: &mut coven::LiveQuery<crate::list::ImportListCount, DbError>,
    what: &str,
    accept: impl Fn(&crate::list::ImportListCount) -> bool,
) -> crate::list::ImportListCount {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let value = live.next().await.expect("the query stays open").unwrap();
            if accept(&value) {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the count reaches {what}"))
}

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_this_device_imports_its_files() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let candidate = scan_finished(&db, root, "release").await;

    let list = open_in_path_order(&db, crate::TriageTab::Pending).await;
    let mut window = tables.subscribe_import_window(list, first(50));
    let mut count = tables.subscribe_import_count(list);
    let initial = window.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial.read.rows), ["release"]);
    assert_eq!(count.next().await.expect("the query stays open").unwrap().rows, 1);
    let initial_counts = tables.load_tab_counts().await.unwrap();
    assert_eq!((initial_counts.pending, initial_counts.done), (1, 0));

    imported_release(&db, &candidate).await;

    let imported =
        next_until(&mut window, "an empty Pending", |read| read.read.rows.is_empty()).await;
    assert!(imported.read.rows.is_empty(), "Pending no longer holds the row");
    assert_eq!(count_until(&mut count, "no rows", |count| count.rows == 0).await.rows, 0);
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
    let list = open_in_path_order(&db, crate::TriageTab::Done).await;
    let read = tables.read_import_window(list, first(50)).await.unwrap();
    assert_eq!(candidate_names(&read.read.rows), vec!["renamed".to_string()]);
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

    let list = open_in_path_order(&db, crate::TriageTab::Done).await;
    let mut count = tables.subscribe_import_count(list);
    assert_eq!(count.next().await.expect("the query stays open").unwrap().rows, 1);

    bae_records::test_fixtures::delete_release(
        &bae_records::test_fixtures::test_records(&db),
        &release.id,
    )
    .await;
    count_until(&mut count, "no rows in Done", |count| count.rows == 0).await;
    assert_eq!(tables.load_tab_counts().await.unwrap().pending, 1);
}

/// Folders scanned into a list of `names`, in path order, and the list.
async fn scanned_list(names: &[&str]) -> (Library, ImportListId) {
    let lib = Library::new().await;
    lib.run(scanned_with_states(&lib, names)).await;
    let list =
        lib.open(ordered(crate::TriageTab::Pending, crate::ImportListOrder::PathAscending)).await;
    (lib, list)
}

/// A window that has been marked to rerun reads what it is marked for, so the
/// next write finds it at rest.
async fn settled<T: Clone + PartialEq + Send + 'static, Request>(
    window: &mut coven::ReconfigurableLiveQuery<Request, T, DbError>,
) where
    Request: Clone + PartialEq + Send + Sync + 'static,
{
    window.next().await.expect("the query stays open");
}

/// A scan item reruns the window whose range holds it and the count, and
/// every other window of the list too: the statements that read the items
/// (`import_list_item`, whose rows show a group's header while any member
/// matches) read the placement through a correlated subquery coven cannot
/// bound by a range, so only the rows the window delivers tell a rerun apart.
/// This pins what coven narrows today; a write the list does not read
/// (`import_list_withholds_a_commit_that_changes_nothing_it_reads`) reruns
/// nothing.
#[tokio::test]
async fn a_scan_item_reruns_the_windows_and_the_count() {
    let (lib, list) = scanned_list(&["A", "B", "C", "D", "E", "F"]).await;
    let tables = lib.tables();
    let middle = tables.import_key_at(list, 3).await.unwrap().unwrap();
    let mut above = tables.subscribe_import_window(list, until(&middle));
    let mut below = tables.subscribe_import_window(list, from(&middle, 50));
    let mut count = tables.subscribe_import_count(list);
    settled(&mut above).await;
    settled(&mut below).await;
    count.next().await.expect("the query stays open").unwrap();

    lib.run(lib.scan("Z")).await;
    assert!(below.is_marked_for_rerun(), "Z lands in the lower window's range");
    assert!(above.is_marked_for_rerun(), "coven bounds neither window's read by its range");
    assert!(count.is_marked_for_rerun());
    let grown = below.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&grown.read.rows).last().map(String::as_str), Some("Z"));
    assert_eq!(count.next().await.expect("the query stays open").unwrap().rows, 7);

    lib.run(lib.scan("A0")).await;
    assert!(above.is_marked_for_rerun(), "A0 lands in the upper window's range");
    let grown = above.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&grown.read.rows), ["A", "A0", "B", "C"]);
}

/// A commit that touches a table the windows do not read does not read them
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

    let list = open_in_path_order(&db, crate::TriageTab::Pending).await;
    let mut window = tables.subscribe_import_window(list, first(50));
    let mut count = tables.subscribe_import_count(list);
    settled(&mut window).await;
    count.next().await.expect("the query stays open").unwrap();

    // What a walk records about the directories it read: the list projects
    // candidates, and never reads this.
    tables.record_folder_scan_directories(root, &[(root.to_path_buf(), 1234)]).await.unwrap();
    assert!(!window.is_marked_for_rerun());
    assert!(!count.is_marked_for_rerun());
}

/// The windows are live queries over the scan tables, so a scan item written
/// while someone is watching wakes the window it lands in. A candidate's rows
/// span several tables; a dependency missed on any of them would leave the
/// list showing the previous scan until something unrelated changed.
#[tokio::test]
async fn import_list_wakes_on_a_scan_item() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &crate::tables::watched_path::host_root("/music");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "first")).await.unwrap();

    let list = open_in_path_order(&db, crate::TriageTab::Pending).await;
    let mut window = tables.subscribe_import_window(list, first(50));
    let initial = window.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(candidate_names(&initial.read.rows), vec!["first".to_string()]);

    tables.save_folder_scan_item(root, generation, &scan_candidate(root, "second")).await.unwrap();
    let grown = next_until(&mut window, "both folders", |read| read.read.rows.len() == 2).await;
    assert_eq!(candidate_names(&grown.read.rows), vec!["first".to_string(), "second".to_string()]);
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

    let list = open_in_path_order(&db, crate::TriageTab::Pending).await;
    let mut window = tables.subscribe_import_window(list, first(50));
    let mut count = tables.subscribe_import_count(list);
    let mut scans = tables.subscribe_scan_found_counts();
    settled(&mut window).await;
    count.next().await.expect("the query stays open").unwrap();
    scans.next().await.expect("the query stays open").unwrap();

    for name in ["first", "second"] {
        tables.save_folder_scan_item(root, rescan, &scan_candidate(root, name)).await.unwrap();
    }
    // The re-confirmed folders move the scan's progress, and nothing the
    // list's windows or count read.
    tokio::time::timeout(HANG_GUARD, scans.next())
        .await
        .expect("the re-confirmed folders move the scan progress")
        .expect("the query stays open")
        .unwrap();
    assert!(!window.is_marked_for_rerun());
    assert!(!count.is_marked_for_rerun());
}

/// Read the list as a chain of windows reads it: each from the last one's
/// `beyond`, `page` items at a time, to the end.
async fn chain(
    tables: &crate::tables::ImportTables,
    list: ImportListId,
    page: u64,
) -> Vec<crate::list::ImportListItem> {
    let mut items = Vec::new();
    let mut start = None;
    loop {
        let window = ListWindow::Forward { from: start.take(), until: None, limit: page };
        let read = tables.read_import_window(list, window).await.unwrap().read;
        items.extend(read.rows);
        match read.beyond {
            Some(beyond) => start = Some(beyond),
            None => return items,
        }
    }
}

/// The whole list as one window reads it.
async fn whole(
    tables: &crate::tables::ImportTables,
    list: ImportListId,
) -> Vec<crate::list::ImportListItem> {
    tables.read_import_window(list, first(u64::MAX)).await.unwrap().read.rows
}

/// A list of grouped folders read as a chain of windows holds every item
/// exactly once, in the order one window over the whole list holds them, in
/// either direction, with the members' headers named; and so it does after a
/// scan adds folders above where the chain started.
#[tokio::test(flavor = "multi_thread")]
async fn a_chain_of_windows_holds_every_item_once_across_a_scan() {
    use crate::{ImportListOrder, ImportListView, PendingFilter};
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    grow_grouped_import_list(&db, tmp.path(), 0, 300).await;
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
    let everything = whole(&tables, list).await;
    assert!(everything.len() > 300, "the groups' headers are items too");
    for page in [7, 10, 64] {
        assert_eq!(chain(&tables, list, page).await, everything, "pages of {page}");
    }
    // Backward from the end, a window at a time.
    let mut backward = Vec::new();
    let mut end = None;
    loop {
        let window = match &end {
            Some(until) => ListWindow::Backward { until: OrderKey::clone(until), limit: 10 },
            None => {
                let last = tables.import_key_at(list, u64::MAX).await.unwrap().unwrap();
                let last_row = tables.read_import_window(list, from(&last, 1)).await.unwrap();
                backward.extend(last_row.read.rows);
                end = Some(last);
                continue;
            }
        };
        let read = tables.read_import_window(list, window).await.unwrap().read;
        end = read.first.clone();
        backward.splice(0..0, read.rows);
        if read.beyond.is_none() {
            break;
        }
    }
    assert_eq!(backward, everything);

    grow_grouped_import_list(&db, tmp.path(), 300, 305).await;
    let grown = whole(&tables, list).await;
    assert!(grown.len() > everything.len());
    assert_eq!(chain(&tables, list, 10).await, grown, "after the scan");
}

/// A text-filtered list read as a chain holds what one window over it holds,
/// to its end: the windows are read from the tab's rows in order, tested one
/// by one, and a jump reads the same rows the chain does.
#[tokio::test(flavor = "multi_thread")]
async fn a_chain_over_a_filtered_list_holds_what_a_jump_reads() {
    use crate::{ImportListOrder, ImportListView, PendingFilter};
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
    let everything = whole(&tables, list).await;
    assert!(everything.len() > 100, "the filter keeps enough rows to scroll through");
    assert_eq!(chain(&tables, list, 10).await, everything);
    for offset in [0, 10, 55, 100, everything.len() as u64 - 1] {
        let key = tables.import_key_at(list, offset).await.unwrap().unwrap();
        let window = tables.read_import_window(list, from(&key, 10)).await.unwrap().read;
        let wanted: Vec<_> = everything.iter().skip(offset as usize).take(10).cloned().collect();
        assert_eq!(window.rows, wanted, "a window from the key at {offset}");
        assert_eq!(
            tables
                .subscribe_import_position(list, key)
                .next()
                .await
                .expect("the query stays open")
                .result
                .unwrap(),
            offset,
            "the position of the key at {offset}"
        );
    }
}

/// A text-filtered list's count, which counts what its search finds, counts
/// again when a scan adds matching folders, when its groups fold and when
/// they open, as a read with nothing kept does.
#[tokio::test(flavor = "multi_thread")]
async fn a_filtered_import_lists_counts_follow_the_scan_and_the_folds() {
    use crate::{ImportListOrder, ImportListView, PendingFilter};
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
    let mut query = tables.subscribe_import_count(list);
    let before = query.next().await.unwrap().unwrap();
    assert_eq!(before.rows, whole(&tables, list).await.len() as u64);
    let mut follows = async |what: &str| {
        let fresh = tables.read_import_list_for_test(list).await.unwrap();
        let fresh =
            crate::list::ImportListCount { rows: fresh.total_count, narrowed: fresh.narrowed };
        count_until(&mut query, &format!("{fresh:?} after {what}"), |count| *count == fresh).await
    };
    tables.set_import_list_groups_folded(list, true).await.unwrap();
    let folded = follows("folding every group").await;
    assert!(
        folded.rows < before.rows,
        "folded groups hide their members: {folded:?} of {before:?}"
    );
    tables.set_import_list_groups_folded(list, false).await.unwrap();
    assert_eq!(follows("opening every group").await, before);
    grow_grouped_import_list(&db, tmp.path(), 100, 200).await;
    let grown = follows("a scan adds matching folders").await;
    assert!(grown.rows > before.rows, "the scan's folders count: {grown:?} of {before:?}");
}

/// A fold changes the windows holding the group and moves the ones below it;
/// the window above it reruns too, as for a scan item, and delivers the rows
/// it held.
#[tokio::test]
async fn a_fold_reruns_the_windows_holding_the_group() {
    let lib = Library::new().await;
    lib.run(scanned_with_states(
        &lib,
        &["A/One", "A/Two", "B/One", "B/Two", "C/One", "C/Two", "Loose"],
    ))
    .await;
    let tables = lib.tables();
    let list =
        lib.open(ordered(crate::TriageTab::Pending, crate::ImportListOrder::PathAscending)).await;
    // Group A and its members, then group B and C's.
    let cut = tables.import_key_at(list, 3).await.unwrap().unwrap();
    let mut above = tables.subscribe_import_window(list, until(&cut));
    let mut below = tables.subscribe_import_window(list, from(&cut, 50));
    settled(&mut above).await;
    settled(&mut below).await;

    tables.set_import_list_group_folded(list, lib.group_key("C"), true).await.unwrap();
    assert!(below.is_marked_for_rerun(), "the window holding the group reruns");
    assert!(above.is_marked_for_rerun(), "coven bounds neither window's read by its range");
    let folded = below.next().await.expect("the query stays open").result.unwrap();
    assert!(!items_sequence(&folded.read.rows).contains(&"candidate C/One".to_string()));
}

/// A selection write reruns the windows holding the rows it changed and the
/// selection summary, and no other window.
#[tokio::test]
async fn a_selection_write_reruns_the_windows_holding_its_rows() {
    let (lib, list) = scanned_list(&["A", "B", "C", "D", "E", "F"]).await;
    let tables = lib.tables();
    let middle = tables.import_key_at(list, 3).await.unwrap().unwrap();
    let mut above = tables.subscribe_import_window(list, until(&middle));
    let mut below = tables.subscribe_import_window(list, from(&middle, 50));
    let mut selection = tables.subscribe_import_selection(list);
    settled(&mut above).await;
    settled(&mut below).await;
    selection.next().await.expect("the query stays open").unwrap();

    lib.select(list, SelectionChange::Replace { keys: vec![lib.key("E")] }).await;
    assert!(below.is_marked_for_rerun(), "the window holding E reruns");
    assert!(selection.is_marked_for_rerun(), "the selection summary reruns");
    assert!(!above.is_marked_for_rerun(), "the window above holds no changed row");
    let selected = below.next().await.expect("the query stays open").result.unwrap();
    assert!(window_rows(&selected).iter().any(|row| row.selected && row.display_path == "E"));
    assert_eq!(selection.next().await.expect("the query stays open").unwrap().count, 1);
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
