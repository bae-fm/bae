//! Reconfiguring the list, and what cancelling it settles.

use super::*;
use crate::db::Database;
use std::collections::HashMap;
use tokio::sync::broadcast;

async fn subscription() -> (ImportListSubscription, tempfile::TempDir) {
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
    let (_changes_tx, changes) = broadcast::channel(8);
    // The sender is dropped with this scope, which the merge task reads as
    // "no more runtime changes" — the same end it reaches when the import
    // service shuts down.
    let subscription = ImportListSubscription::start(
        query,
        request,
        changes,
        HashMap::new,
        &tokio::runtime::Handle::current(),
    );
    (subscription, tmp)
}

/// The windows travel in the request, so asking for one reruns the query and
/// the value says it was the request that changed.
#[tokio::test]
async fn setting_the_windows_reruns_the_query_as_a_request_change() {
    let (subscription, _tmp) = subscription().await;

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
    let (subscription, _tmp) = subscription().await;
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
