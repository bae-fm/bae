//! Reconfiguring the list, and what cancelling it settles.

use super::*;
use crate::db::Database;
use crate::import::{
    CandidateRuntimeChange, CandidateRuntimeSnapshot, ImportInFlight, ImportPhase, ImportStep,
};
use std::collections::HashMap;
use tokio::sync::broadcast;

/// The subscription, and the runtime stream behind it — held by the caller so
/// the merge task stays open for as long as the test wants to feed it.
async fn subscription() -> (
    ImportListSubscription,
    broadcast::Sender<CandidateRuntimeChange>,
    tempfile::TempDir,
) {
    let tmp = tempfile::TempDir::new().expect("a temp library dir");
    let database = Database::new_test(
        tmp.path()
            .join("test.db")
            .to_str()
            .expect("a UTF-8 temp path"),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("the test database opens");
    let request = ImportListRequest::default();
    let query = database.subscribe_import_list(request.clone());
    let (changes_tx, changes) = broadcast::channel(8);
    // Dropping the sender is what the merge task reads as "no more runtime
    // changes" — the same end it reaches when the import service shuts down.
    // Nothing publishes an outbox snapshot here, and dropping the sender ends
    // the upload-standing merge the way library shutdown does. These tests are
    // about the runtime merge and the request round trip.
    let (_outbox_tx, outbox) = tokio::sync::watch::channel(None);
    let subscription = ImportListSubscription::start(
        query,
        request,
        changes,
        HashMap::new,
        outbox,
        &tokio::runtime::Handle::current(),
    );
    (subscription, changes_tx, tmp)
}

/// The windows travel in the request, so asking for one reruns the query and
/// the value says it was the request that changed.
#[tokio::test]
async fn setting_the_windows_reruns_the_query_as_a_request_change() {
    let (subscription, _changes, _tmp) = subscription().await;

    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(initial.cause, coven::ReconfigurableLiveQueryCause::Initial);
    assert!(initial.windows.is_empty());

    subscription
        .set_windows(
            std::iter::once(LibraryPageWindow {
                offset: 0,
                limit: 50,
            })
            .collect(),
        )
        .expect("the first window is requested");
    let requested = subscription.next().await.expect("the requested value");
    assert_eq!(
        requested.cause,
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(requested.windows.len(), 1);
    assert_eq!(requested.request_revision, 1);
}

/// Cancelling settles a pending read and refuses every later reconfiguration:
/// the list is gone, not merely quiet.
#[tokio::test]
async fn cancelling_refuses_a_later_view_change() {
    let (subscription, _changes, _tmp) = subscription().await;
    subscription.next().await.expect("the initial value");

    subscription.cancel().await;

    assert!(matches!(
        subscription.set_view(ImportListView::default()),
        Err(ImportListSubscriptionError::Cancelled)
    ));
    assert!(matches!(
        subscription.next().await,
        Err(ImportListSubscriptionError::Cancelled)
    ));
}

/// A running import ticks by the second, and none of those ticks moves a fact
/// a row's placement reads — so the standing request is left alone and the
/// query does not rerun. Claiming the candidate and the import ending both do
/// move one, and both rerun it.
#[tokio::test]
async fn only_a_change_that_moves_a_placement_reruns_the_query() {
    let (subscription, changes, _tmp) = subscription().await;
    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(initial.request_revision, 0);

    let key = "/music/Release".to_string();
    let claimed = CandidateRuntimeSnapshot {
        identify: None,
        import: Some(ImportInFlight {
            progress_percent: None,
            step: None,
        }),
        search: None,
    };
    changes
        .send(CandidateRuntimeChange::Updated {
            key: key.clone(),
            runtime: claimed,
        })
        .expect("the merge task is listening");
    let claimed = subscription.next().await.expect("the claim reruns");
    assert_eq!(
        claimed.cause,
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(claimed.request_revision, 1);

    // Two ticks of the same import, then the import ending. If a tick had
    // reconfigured anything, the ending's revision would be past 2.
    for percent in [40, 80] {
        changes
            .send(CandidateRuntimeChange::Updated {
                key: key.clone(),
                runtime: CandidateRuntimeSnapshot {
                    identify: None,
                    import: Some(ImportInFlight {
                        progress_percent: Some(percent),
                        step: Some(ImportStep::Running(ImportPhase::MeasuringLoudness)),
                    }),
                    search: None,
                },
            })
            .expect("the merge task is listening");
    }
    changes
        .send(CandidateRuntimeChange::Removed { key })
        .expect("the merge task is listening");

    let ended = subscription.next().await.expect("the import ending reruns");
    assert_eq!(
        ended.cause,
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(ended.request_revision, 2);
}
