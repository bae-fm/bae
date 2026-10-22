//! Reconfiguring the list, and what cancelling it settles.

use super::*;
use crate::db::Database;

/// The subscription over an empty queue. Nothing publishes an outbox snapshot
/// here; these tests are about the request round trip.
async fn subscription() -> (ImportListSubscription, Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("a temp library dir");
    let database = Database::new_test(
        tmp.path()
            .join("test.db")
            .to_str()
            .expect("a UTF-8 temp path"),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("the test database opens");
    let (_outbox_tx, outbox) = tokio::sync::watch::channel(None);
    let request = ImportListRequest {
        upload_status: UploadStatus::of(&outbox.borrow()),
        ..ImportListRequest::default()
    };
    let query = database.subscribe_import_list(request.clone());
    let subscription = ImportListSubscription::start(
        query,
        database.subscribe_folder_scan_progress(),
        request,
        outbox,
        crate::import::candidate_runtime::RuntimeFactsWatch::of(&Default::default()),
        &tokio::runtime::Handle::current(),
    );
    (subscription, database, tmp)
}

/// The windows travel in the request, so asking for one reruns the query and
/// the value says it was the request that changed.
#[tokio::test]
async fn setting_the_windows_reruns_the_query_as_a_request_change() {
    let (subscription, _database, _tmp) = subscription().await;

    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(initial.cause, coven::LiveQueryCause::Request);
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
        coven::LiveQueryCause::Request
    );
    assert_eq!(requested.windows.len(), 1);
    assert_eq!(requested.request_revision, 1);
}

/// Cancelling settles a pending read and refuses every later reconfiguration:
/// the list is gone, not merely quiet.
#[tokio::test]
async fn cancelling_refuses_a_later_view_change() {
    let (subscription, _database, _tmp) = subscription().await;
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

/// The list reads the upload queue the way its request opened: waiting until
/// the queue first answers, and unreadable once a read of it fails, never
/// keeping an order the queue no longer backs.
#[tokio::test]
async fn a_failed_upload_queue_read_makes_the_done_order_unreadable() {
    let tmp = tempfile::TempDir::new().expect("a temp library dir");
    let database = Database::new_test(
        tmp.path()
            .join("test.db")
            .to_str()
            .expect("a UTF-8 temp path"),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("the test database opens");
    let (outbox_tx, outbox) = tokio::sync::watch::channel(None);
    let request = ImportListRequest {
        view: ImportListView {
            tab: crate::import::triage::TriageTab::Done,
            ..ImportListView::default()
        },
        upload_status: UploadStatus::of(&outbox.borrow()),
        ..ImportListRequest::default()
    };
    let subscription = ImportListSubscription::start(
        database.subscribe_import_list(request.clone()),
        database.subscribe_folder_scan_progress(),
        request,
        outbox,
        crate::import::candidate_runtime::RuntimeFactsWatch::of(&Default::default()),
        &tokio::runtime::Handle::current(),
    );

    let initial = subscription.next().await.expect("the initial value");
    assert_eq!(
        initial.summary.done_upload_status,
        Some(UploadStatusKnown::Waiting)
    );

    outbox_tx.send_replace(Some(Err("the queue's table is locked".to_string())));
    let unreadable = subscription.next().await.expect("the value after the failed read");
    assert_eq!(
        unreadable.summary.done_upload_status,
        Some(UploadStatusKnown::Unreadable)
    );
}
