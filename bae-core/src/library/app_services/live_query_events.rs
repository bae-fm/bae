//! Live queries owned by a task of their own, so a subscription that merges
//! one with other streams never cancels the query's read.

/// A live query's events, produced on a task of the query's own.
///
/// A subscription that merges a live query with other streams must take the
/// query's events from here rather than poll [`coven::LiveQuery::next`] in its
/// `select!` directly. `select!` drops the branches it does not pick, and
/// dropping `next` throws away the database read it had in flight — the run
/// stays pending, so the next poll starts that read over from the beginning. A
/// steady stream on the other branches is what a running sync cycle looks like
/// from here, and it restarts the read again and again; on a device where the
/// read takes longer than the gap between those events the query never finishes
/// its first run at all, and the screen waiting on its first value gets neither
/// a value nor an error. Owning the query in its own task puts its read out of
/// reach of the merge loop, whose only query branch is then a channel receive,
/// which loses nothing when it is dropped and polled again.
///
/// Dropping this stops that task, so a subscription that ends — or replaces its
/// query — takes the query it is done with down too.
pub(super) struct LiveQueryEvents<T> {
    events: tokio::sync::mpsc::UnboundedReceiver<Result<T, crate::library::LibraryError>>,
    task: tokio::task::JoinHandle<()>,
}

impl<T> LiveQueryEvents<T> {
    pub(super) async fn recv(&mut self) -> Option<Result<T, crate::library::LibraryError>> {
        self.events.recv().await
    }
}

impl<T> Drop for LiveQueryEvents<T> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn live_query_events<T>(
    runtime_handle: &tokio::runtime::Handle,
    mut query: coven::LiveQuery<T>,
) -> LiveQueryEvents<T>
where
    T: Clone + PartialEq + Send + 'static,
{
    let (tx, events) = tokio::sync::mpsc::unbounded_channel();
    let task = runtime_handle.spawn(async move {
        loop {
            let event = query.next().await.map_err(live_query_error);
            if tx.send(event).is_err() {
                return;
            }
        }
    });
    LiveQueryEvents { events, task }
}

fn live_query_error(error: coven::CovenError) -> crate::library::LibraryError {
    crate::library::LibraryError::Database(match error {
        coven::CovenError::Database(error) => *error,
        other => coven::DbError::Message(other.to_string()),
    })
}

/// [`LiveQueryEvents`] for a query whose request changes while it runs: the
/// query lives on its own task, and [`set`](Self::set) points the same query at
/// a new request instead of opening another one.
///
/// Every subscriber through here shows only the newest request, so
/// [`recv`](Self::recv) hands over only events that answer the request last
/// set. Coven delivers a read that finished after its request was replaced —
/// that is what keeps a request changing faster than one read from starving
/// the query — and marks it with the revision it answered; one older than the
/// revision [`set`](Self::set) got back is skipped here, and the read for the
/// newest request follows.
pub(super) struct ReconfigurableLiveQueryEvents<Request, T> {
    events: tokio::sync::mpsc::UnboundedReceiver<(
        coven::LiveQueryRevision,
        Result<T, crate::library::LibraryError>,
    )>,
    requests: coven::LiveQueryRequests<Request>,
    /// The revision of the request last set; events answering an older one
    /// are not delivered. `None` while the initial request is the only one.
    newest: Option<coven::LiveQueryRevision>,
    task: tokio::task::JoinHandle<()>,
}

impl<Request, T> ReconfigurableLiveQueryEvents<Request, T>
where
    Request: Clone + PartialEq,
{
    /// The next result for the newest request, or `None` once the query ends.
    pub(super) async fn recv(&mut self) -> Option<Result<T, crate::library::LibraryError>> {
        loop {
            let (revision, result) = self.events.recv().await?;
            if self.newest.is_none_or(|newest| revision == newest) {
                return Some(result);
            }
        }
    }

    pub(super) fn set(&mut self, request: Request) {
        self.newest = Some(
            self.requests
                .set(request)
                .expect("the query task retains its subscription while this handle lives"),
        );
    }
}

impl<Request, T> Drop for ReconfigurableLiveQueryEvents<Request, T> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn reconfigurable_live_query_events<Request, T>(
    runtime_handle: &tokio::runtime::Handle,
    mut query: coven::ReconfigurableLiveQuery<Request, T>,
) -> ReconfigurableLiveQueryEvents<Request, T>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    let requests = query.requests();
    let (tx, events) = tokio::sync::mpsc::unbounded_channel();
    let task = runtime_handle.spawn(async move {
        loop {
            let event = query.next().await;
            let revision = event.revision();
            if tx
                .send((revision, event.into_result().map_err(live_query_error)))
                .is_err()
            {
                return;
            }
        }
    });
    ReconfigurableLiveQueryEvents {
        events,
        requests,
        newest: None,
        task,
    }
}
