//! Live queries owned by a task of their own, so a subscription that merges
//! one with other streams never cancels the query's read.

use tracing::warn;

/// A live query's events, produced on a task of the query's own, whose
/// request changes while it runs: [`set`](Self::set) points the same query at
/// a new request instead of opening another one.
///
/// A subscription that merges a live query with other streams must take the
/// query's events from here rather than poll the query's `next` in its
/// `select!` directly. `select!` drops the branches it does not pick, and
/// dropping `next` throws away the database read it had in flight — the run
/// stays pending, so the next poll starts that read over from the beginning. A
/// steady stream on the other branches is what a running sync cycle looks like
/// from here, and it restarts the read again and again; on a device where the
/// read takes longer than the gap between those events the query never finishes
/// its first run at all, and the screen waiting on its first value gets neither
/// a value nor an error. Owning the query in its own task puts its read out of
/// reach of the merge loop, whose only query branch is then a channel receive,
/// which loses nothing when it is dropped and polled again. Dropping this stops
/// that task, so a subscription that ends takes its query down too. The task
/// is one of the library's, so closing the library ends it before the store
/// closes.
///
/// Every subscriber through here shows only the newest request, so
/// [`recv`](Self::recv) hands over only events that answer the request last
/// set. Coven delivers a read that finished after its request was replaced —
/// that is what keeps a request changing faster than one read from starving
/// the query — and marks it with the revision it answered; one older than the
/// revision [`set`](Self::set) got back is skipped here, and the read for the
/// newest request follows.
pub struct ReconfigurableLiveQueryEvents<Request, T> {
    events: tokio::sync::mpsc::UnboundedReceiver<(
        coven::LiveQueryRevision,
        coven::LiveQueryCause,
        Result<T, crate::LibraryError>,
    )>,
    requests: coven::LiveQueryRequests<Request>,
    /// The revision of the request last set; events answering an older one
    /// are not delivered. `None` while the initial request is the only one.
    newest: Option<coven::LiveQueryRevision>,
    /// Stops the task that owns the query.
    task: crate::CancellationToken,
}

impl<Request, T> ReconfigurableLiveQueryEvents<Request, T>
where
    Request: Clone + PartialEq + std::fmt::Debug,
{
    /// The next result for the newest request, or `None` once the query ends.
    pub async fn recv(&mut self) -> Option<Result<T, crate::LibraryError>> {
        self.recv_with_cause().await.map(|(_, result)| result)
    }

    /// [`Self::recv`] with why the query ran: a new request, a write to what
    /// it read, or both.
    pub async fn recv_with_cause(
        &mut self,
    ) -> Option<(coven::LiveQueryCause, Result<T, crate::LibraryError>)> {
        loop {
            let (revision, cause, result) = self.events.recv().await?;
            if self.newest.is_none_or(|newest| revision == newest) {
                return Some((cause, result));
            }
        }
    }

    /// Point the query at `request`. A query that ended, once its store
    /// closed or its handle failed, reads nothing more, for this request or
    /// any later one, and [`Self::recv`] has ended with it.
    pub fn set(&mut self, request: Request) {
        match self.requests.set(request.clone()) {
            Ok(revision) => self.newest = Some(revision),
            Err(coven::LiveQueryClosed) => {
                warn!("the live query has ended; {request:?} is not read");
            }
        }
    }
}

impl<Request, T> Drop for ReconfigurableLiveQueryEvents<Request, T> {
    fn drop(&mut self) {
        self.task.cancel();
    }
}

/// `reader` is the subscription these events feed: a panic in the task that
/// owns the query ends that subscription with the failure, as a panic in the
/// subscription's own task does.
pub fn reconfigurable_live_query_events<Request, T>(
    manager: &crate::LibraryManager,
    mut query: coven::ReconfigurableLiveQuery<Request, T, coven::CovenError>,
    reader: impl crate::SubscriptionReader,
) -> ReconfigurableLiveQueryEvents<Request, T>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    let requests = query.requests();
    let (tx, events) = tokio::sync::mpsc::unbounded_channel();
    let task = crate::CancellationToken::new();
    manager.spawn_stoppable_subscription_task(
        "live query events",
        task.clone(),
        reader,
        async move {
            while let Some(event) = query.next().await {
                let revision = event.revision;
                if tx
                    .send((revision, event.cause, event.result.map_err(crate::LibraryError::from)))
                    .is_err()
                {
                    return;
                }
            }
        },
    );
    ReconfigurableLiveQueryEvents { events, requests, newest: None, task }
}

#[cfg(test)]
#[path = "live_query_events_tests.rs"]
mod tests;
