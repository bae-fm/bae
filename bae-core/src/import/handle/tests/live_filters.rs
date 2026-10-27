//! In Progress follows where each candidate stands: a row shows while its run
//! or import is queued or going, and leaves when it ends — and leaves the
//! entry the tables put it under for as long. Every snapshot counts the
//! filter's entries with each row's standing, and a change that moves a count
//! off the page delivers one.

use super::*;
use crate::import::{ImportListSubscription, ImportListView, PendingFilter};

/// The list under one filter entry, and the keys and filter entries its last
/// snapshot carried.
struct LiveList {
    list: ImportListSubscription,
    shown: Option<Vec<String>>,
    entries: Vec<crate::import::PendingFilterEntry>,
}

impl LiveList {
    async fn new(handle: &ImportServiceHandle, filter: PendingFilter) -> Self {
        Self {
            list: handle
                .subscribe_whole_list(ImportListView {
                    pending_filter: filter,
                    ..ImportListView::default()
                })
                .await,
            shown: None,
            entries: Vec::new(),
        }
    }

    /// Wait until the list shows exactly `expected`; at once when it already
    /// does, since a change that moves no row delivers nothing.
    async fn shows(&mut self, expected: &[&str]) {
        let wanted: Vec<String> = expected.iter().map(|key| key.to_string()).collect();
        let reached = tokio::time::timeout(HANG_GUARD, async {
            while self.shown.as_ref() != Some(&wanted) {
                self.next().await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "the list shows {:?}, not {wanted:?}",
            self.shown
        );
    }

    /// How many rows the last snapshot counts under `filter`; none before
    /// the first.
    fn counted(&self, filter: PendingFilter) -> Option<u32> {
        counted(&self.entries, filter)
    }

    /// Wait until a snapshot counts `count` rows under `filter`; at once when
    /// the last one does.
    async fn counts(&mut self, filter: PendingFilter, count: u32) {
        let reached = tokio::time::timeout(HANG_GUARD, async {
            while self.counted(filter) != Some(count) {
                self.next().await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "the list counts {:?} under {filter:?}, not {count}",
            self.counted(filter)
        );
    }

    async fn next(&mut self) {
        let snapshot = self.list.next().await.expect("the list answers");
        self.entries = snapshot.pending_filter_entries.clone();
        self.shown = Some(
            snapshot
                .windows
                .iter()
                .flat_map(|window| &window.items)
                .filter_map(|item| match item {
                    crate::import::ImportListItem::Candidate { row, .. } => {
                        Some(row.candidate_key.clone())
                    }
                    _ => None,
                })
                .collect(),
        );
    }
}

/// An import shows under In Progress while it waits for the worker and while
/// it runs, and leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn importing_shows_a_queued_and_a_running_import_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress).await;
    list.shows(&[]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[&key]).await;

    handle
        .runtime
        .move_import(&key, "import-1", Some(crate::import::ImportStanding::Running))
        .await
        .unwrap();
    list.shows(&[&key]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.shows(&[]).await;
    shut_down(handle).await;
}

/// A run shows under In Progress while it is queued and while it runs, and
/// leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn identifying_shows_a_queued_and_a_running_run_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress).await;
    list.shows(&[]).await;

    admit(&handle, &key).await;
    list.shows(&[&key]).await;

    start_run(&handle, &key, crate::identify::IdentifyRunId::for_test(1)).await;
    list.shows(&[&key]).await;

    handle.cancel_identification(&key).await.unwrap();
    list.shows(&[]).await;
    shut_down(handle).await;
}

/// A row an import owns is In Progress, not under the entry the tables put
/// it under: it leaves that entry while the import goes and is back once it
/// ends, so no row is ever under two entries past All.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_leaves_its_stored_entry_while_an_import_owns_it() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let stored = handle
        .candidate_pane(&key)
        .await
        .unwrap()
        .expect("the candidate reads back")
        .live
        .standing
        .expect("the candidate is on Found")
        .state();
    // The entry past All Found's menu holds the state under.
    let entry = match stored {
        crate::import::PendingState::NeedsYou
        | crate::import::PendingState::LookupError
        | crate::import::PendingState::Error
        | crate::import::PendingState::ImportError => PendingFilter::NeedsYou,
        crate::import::PendingState::Identifying | crate::import::PendingState::Importing => {
            PendingFilter::InProgress
        }
        crate::import::PendingState::Identified => PendingFilter::Identified,
        crate::import::PendingState::Unmatched => PendingFilter::Unmatched,
        crate::import::PendingState::NotLookedUp => PendingFilter::NotLookedUp,
    };
    assert_ne!(entry, PendingFilter::InProgress);
    let mut list = LiveList::new(&handle, entry).await;
    list.shows(&[&key]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.shows(&[&key]).await;
    shut_down(handle).await;
}

/// The filter's counts follow what is running: a claimed import moves its
/// row's count from the entry the tables put it under to In
/// Progress, and back once the import ends, with the list under All showing
/// every row throughout.
#[tokio::test(flavor = "multi_thread")]
async fn the_entry_counts_follow_what_is_running() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::All).await;
    list.shows(&[&key]).await;
    list.counts(PendingFilter::All, 1).await;
    list.counts(PendingFilter::InProgress, 0).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.counts(PendingFilter::InProgress, 1).await;
    list.counts(PendingFilter::All, 1).await;
    list.shows(&[&key]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.counts(PendingFilter::InProgress, 0).await;
    list.counts(PendingFilter::All, 1).await;
    shut_down(handle).await;
}

/// A watched root holding two stored candidates, each with its draft read
/// from its own tags, and the list under All through a window that holds the
/// first item alone: the key on that page, and the key off it.
async fn one_row_on_the_page() -> (ImportServiceHandle, TempDir, ImportListSubscription, String, String) {
    let (manager, tmp) = setup_test_manager_with(bae_util::http::Http::for_test()).await;
    let (first, first_key, _) = unscanned_candidate(&manager, &tmp, "Album").await;
    let (second, second_key, _) = unscanned_candidate(&manager, &tmp, "Other Record").await;
    let root = first.watched_folder_path.clone();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    for candidate in [first, second] {
        manager
            .save_folder_scan_item(
                &root,
                generation,
                &crate::import::folder_scanner::ScanItem::Valid(candidate),
            )
            .await
            .unwrap();
    }
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    for key in [&first_key, &second_key] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }
    let list = handle
        .subscribe_list_window(
            ImportListView::default(),
            crate::library::LibraryPageWindow {
                offset: 0,
                limit: 1,
            },
        )
        .await;
    let snapshot = delivered(&list).await;
    let on_page: Vec<String> = snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(candidate_key)
        .map(str::to_string)
        .collect();
    assert_eq!(on_page.len(), 1, "the page holds one row: {on_page:?}");
    let entries = &snapshot.pending_filter_entries;
    assert_eq!(counted(entries, PendingFilter::All), Some(2));
    assert_eq!(counted(entries, PendingFilter::InProgress), Some(0));
    assert_eq!(counted(entries, PendingFilter::Identified), Some(0));
    let on_page = on_page.into_iter().next().expect("one row");
    let off_page = if on_page == first_key { second_key } else { first_key };
    (handle, tmp, list, on_page, off_page)
}

/// The candidate whose row `item` is.
fn candidate_key(item: &crate::import::ImportListItem) -> Option<&str> {
    match item {
        crate::import::ImportListItem::Candidate { row, .. } => Some(&row.candidate_key),
        crate::import::ImportListItem::GroupHeader { .. }
        | crate::import::ImportListItem::Invalid { .. } => None,
    }
}

/// The next snapshot the list delivers.
async fn delivered(list: &ImportListSubscription) -> crate::import::ImportListSnapshot {
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// How many rows `entries` count under `filter`; `None` when they hold no
/// entry for it, as before the list's first snapshot.
fn counted(entries: &[crate::import::PendingFilterEntry], filter: PendingFilter) -> Option<u32> {
    entries
        .iter()
        .find(|entry| entry.filter == filter)
        .map(|entry| entry.count)
}

/// A run starting for a row off the page, under All, changes nothing on the
/// page but moves the row's count to In Progress, so the list delivers it.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_off_the_page_delivers_its_count() {
    let (handle, _tmp, list, on_page, off_page) = one_row_on_the_page().await;

    admit(&handle, &off_page).await;
    let moved = delivered(&list).await;

    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::InProgress), Some(1));
    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::All), Some(2));
    assert_eq!(
        moved
            .windows
            .iter()
            .flat_map(|window| &window.items)
            .filter_map(candidate_key)
            .collect::<Vec<_>>(),
        vec![on_page.as_str()],
        "the page holds the same row"
    );
    shut_down(handle).await;
}

/// A pick for a row off the page moves its count to Identified, so the list
/// delivers it.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_its_count() {
    let (handle, _tmp, list, _on_page, off_page) = one_row_on_the_page().await;
    super::lookup_choices::seed_mb_release_with_catalog(
        handle.library_manager.providers(),
        "off-the-page",
        "CAT-1",
    );

    super::lookup_choices::pick(&handle, &off_page, "off-the-page").await;
    // The pick's write and its cancelling the row's run each deliver; while
    // the run still goes it counts the row under In Progress.
    let moved = loop {
        let snapshot = delivered(&list).await;
        if counted(&snapshot.pending_filter_entries, PendingFilter::Identified) == Some(1) {
            break snapshot;
        }
    };

    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::Identified), Some(1));
    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::All), Some(2));
    shut_down(handle).await;
}
