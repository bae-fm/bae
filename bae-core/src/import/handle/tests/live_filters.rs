//! The Identifying and Importing filters follow the candidate runtime: a row
//! shows while its run or import is queued or going, and leaves when it ends.

use super::*;
use crate::import::{ImportListSubscription, ImportListView, PendingFilter};

/// The list under a live filter, and the keys its last snapshot showed.
struct LiveList {
    list: ImportListSubscription,
    shown: Option<Vec<String>>,
}

impl LiveList {
    fn new(handle: &ImportServiceHandle, filter: PendingFilter) -> Self {
        Self {
            list: handle.subscribe_whole_list(ImportListView {
                pending_filter: Some(filter),
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
        })
        .await;
        assert!(
            reached.is_ok(),
            "the list shows {:?}, not {wanted:?}",
            self.shown
        );
    }
}

fn import_event(key: &str, progress: crate::import::ImportProgress) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress,
    }
}

/// An import shows under Importing while it waits for the worker and while it
/// runs, and leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn importing_shows_a_queued_and_a_running_import_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::Importing);
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

/// A run shows under Identifying while it is queued and while it runs, and
/// leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn identifying_shows_a_queued_and_a_running_run_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::Identifying);
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
