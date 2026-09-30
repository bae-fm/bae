//! In Progress follows the candidate runtime: a row shows while its run or
//! import is queued or going, and leaves when it ends — and leaves the entry
//! the tables put it under for as long. The filter menu, opened at any
//! point, counts what is running then.

use super::*;
use crate::import::{ImportListSubscription, ImportListView, PendingFilter};

/// The list under one filter entry, and the keys its last snapshot showed.
struct LiveList {
    list: ImportListSubscription,
    shown: Option<Vec<String>>,
}

impl LiveList {
    fn new(handle: &ImportServiceHandle, filter: PendingFilter) -> Self {
        Self {
            list: handle.subscribe_whole_list(ImportListView {
                pending_filter: filter,
                ..ImportListView::default()
            }),
            shown: None,
        }
    }

    /// Wait until the list shows exactly `expected`; at once when it already
    /// does, since a change that moves no row delivers nothing.
    async fn shows(&mut self, expected: &[&str]) {
        let wanted: Vec<String> = expected.iter().map(|key| key.to_string()).collect();
        let reached = tokio::time::timeout(std::time::Duration::from_secs(5), async {
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

    /// How many rows the filter menu, opened now, counts under `filter`.
    fn counted(&self, filter: PendingFilter) -> u32 {
        self.list
            .pending_filter_entries()
            .into_iter()
            .find(|entry| entry.filter == filter)
            .expect("every entry is counted")
            .count
    }

    /// Wait until the filter menu counts `count` rows under `filter`. What is
    /// running reaches the subscription before the page it changes is
    /// delivered again, so each delivery is a point to open the menu at.
    async fn counts(&mut self, filter: PendingFilter, count: u32) {
        let reached = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while self.counted(filter) != count {
                self.next().await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "the menu counts {} under {filter:?}, not {count}",
            self.counted(filter)
        );
    }

    async fn next(&mut self) {
        let snapshot = self.list.next().await.expect("the list answers");
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

fn import_event(key: &str, progress: crate::import::ImportProgress) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress,
    }
}

/// An import shows under In Progress while it waits for the worker and while
/// it runs, and leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn importing_shows_a_queued_and_a_running_import_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress);
    list.shows(&[]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[&key]).await;

    handle.event_tx.send(import_event(
        &key,
        crate::import::ImportProgress::Preparing {
            import_id: "import-1".to_string(),
            step: crate::import::PrepareStep::ValidatingSourceFiles,
            album_title: String::new(),
            artist_name: String::new(),
        },
    ));
    list.shows(&[&key]).await;

    handle.event_tx.send(import_event(
        &key,
        crate::import::ImportProgress::Cancelled {
            import_id: "import-1".to_string(),
        },
    ));
    list.shows(&[]).await;
    shut_down(handle).await;
}

/// A run shows under In Progress while it is queued and while it runs, and
/// leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn identifying_shows_a_queued_and_a_running_run_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress);
    list.shows(&[]).await;

    handle.admit_identification(vec![key.clone()], crate::import::Admission::Requested);
    list.shows(&[&key]).await;

    handle.event_tx.send(ImportEvent::IdentifyStateChanged {
        candidate_key: key.clone(),
        run: crate::identify::IdentifyRunId::for_test(1),
        state: crate::import::candidate_runtime::tests::triangulating(),
        priority: crate::util::rate_limiter::CallPriority::Interactive,
    });
    list.shows(&[&key]).await;

    handle.cancel_identification(&key);
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
    let entry = PendingFilter::ENTRIES
        .into_iter()
        .find(|&entry| entry != PendingFilter::All && entry.holds(stored))
        .expect("every state is under an entry past All");
    assert_ne!(entry, PendingFilter::InProgress);
    let mut list = LiveList::new(&handle, entry);
    list.shows(&[&key]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[]).await;

    handle.event_tx.send(import_event(
        &key,
        crate::import::ImportProgress::Cancelled {
            import_id: "import-1".to_string(),
        },
    ));
    list.shows(&[&key]).await;
    shut_down(handle).await;
}

/// The filter menu counts what is running when it opens: a claimed import
/// moves its row's count from the entry the tables put it under to In
/// Progress, and back once the import ends, with the list under All showing
/// every row throughout.
#[tokio::test(flavor = "multi_thread")]
async fn the_entry_counts_follow_what_is_running() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::All);
    list.shows(&[&key]).await;
    list.counts(PendingFilter::All, 1).await;
    list.counts(PendingFilter::InProgress, 0).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.counts(PendingFilter::InProgress, 1).await;
    list.counts(PendingFilter::All, 1).await;
    list.shows(&[&key]).await;

    handle.event_tx.send(import_event(
        &key,
        crate::import::ImportProgress::Cancelled {
            import_id: "import-1".to_string(),
        },
    ));
    list.counts(PendingFilter::InProgress, 0).await;
    list.counts(PendingFilter::All, 1).await;
    shut_down(handle).await;
}
