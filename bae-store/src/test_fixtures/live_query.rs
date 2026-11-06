//! How a test checks what moved a live query.

use crate::LibraryPageWindow;
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

/// Move `live` to `request` and return the event answering it, asserting the
/// move alone caused that run. Every commit since the query's last value is
/// read into the same run, so one touching anything the query reads makes the
/// cause `RequestAndDatabaseChanged`.
pub async fn next_run_is_only_the_request<Request, Value>(
    live: &mut coven::ReconfigurableLiveQuery<Request, Value, crate::DbError>,
    request: Request,
) -> coven::ReconfigurableLiveQueryEvent<Request, Value, crate::DbError>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Clone + PartialEq + Send + 'static,
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

/// The first page of a list, as the one window a browse query reads.
pub fn first_page() -> BTreeSet<LibraryPageWindow> {
    [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect()
}
