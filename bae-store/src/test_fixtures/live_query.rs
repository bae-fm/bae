//! How a test checks what moved a live query.

use crate::list_windows::OrderKey;
use crate::ListWindow;
use bae_util::test_channels::HANG_GUARD;

/// Move `live` to `request` and return the event answering it, asserting the
/// move alone caused that run. Every commit since the query's last value is
/// read into the same run, so one touching anything the query reads makes the
/// cause `RequestAndDatabaseChanged`.
pub async fn next_run_is_only_the_request<Request, Value, E>(
    live: &mut coven::ReconfigurableLiveQuery<Request, Value, E>,
    request: Request,
) -> coven::ReconfigurableLiveQueryEvent<Request, Value, E>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
{
    let revision = live.requests().set(request).expect("the subscription is open");
    let event = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a new request reruns the query")
        .expect("the query stays open");
    assert_eq!(event.revision, revision);
    assert_eq!(
        event.cause,
        coven::LiveQueryCause::Request,
        "a commit since the last value touched what the query reads"
    );
    event
}

/// The window of a list from its top to its end, as one window of up to 50
/// rows.
pub fn top_window() -> ListWindow {
    ListWindow::Forward { from: None, until: None, limit: 50 }
}

/// Three windows sharing bounds at `second` and `fourth`: the rows above
/// `second`, those from `second` up to `fourth`, and those from `fourth` on.
pub fn windows_split_at(second: OrderKey, fourth: OrderKey) -> [ListWindow; 3] {
    [
        ListWindow::Forward { from: None, until: Some(second.clone()), limit: 50 },
        ListWindow::Forward { from: Some(second), until: Some(fourth.clone()), limit: 50 },
        ListWindow::Forward { from: Some(fourth), until: None, limit: 50 },
    ]
}
