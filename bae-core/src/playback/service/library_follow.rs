//! A live library read the run loop keeps pointed at what the service is
//! playing, and wakes on when the library changes what it reads.
//!
//! The loop hands it its request every turn, like the side-pause countdown
//! wait: an unchanged request keeps the query, a new one re-points it. The
//! query runs on a task of its own, so the loop's `select!` dropping the
//! branch never throws away a read in flight; the loop's only branch is a
//! channel receive.

use tracing::error;

pub(super) struct LibraryFollow<Request, Value> {
    requests: coven::LiveQueryRequests<Request>,
    request: Request,
    reads: tokio::sync::mpsc::UnboundedReceiver<(Request, Value)>,
    task: tokio::task::JoinHandle<()>,
}

impl<Request, Value> LibraryFollow<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + std::fmt::Debug + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    /// Follow `query` on a task of its own. A read that fails is logged under
    /// `what` and the follow keeps running: the next change reads again.
    pub(super) fn new(
        mut query: coven::ReconfigurableLiveQuery<Request, Value>,
        request: Request,
        what: &'static str,
    ) -> Self {
        let requests = query.requests();
        let (tx, reads) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                let event = query.next().await;
                let request = event.request().clone();
                match event.into_result() {
                    Ok(value) => {
                        if tx.send((request, value)).is_err() {
                            return;
                        }
                    }
                    Err(error) => error!("reading {what} for {request:?} failed: {error}"),
                }
            }
        });
        Self {
            requests,
            request,
            reads,
            task,
        }
    }

    /// Point the read at `request`, unless it already reads it.
    pub(super) fn follow(&mut self, request: Request) {
        if request != self.request {
            self.requests
                .set(request.clone())
                .expect("the follow task holds its query while this handle lives");
            self.request = request;
        }
    }

    /// The next value read for the current request. Reads that answer an
    /// earlier request are skipped.
    pub(super) async fn next(&mut self) -> (Request, Value) {
        loop {
            let Some((request, value)) = self.reads.recv().await else {
                return std::future::pending().await;
            };
            if request == self.request {
                return (request, value);
            }
        }
    }
}

impl<Request, Value> Drop for LibraryFollow<Request, Value> {
    fn drop(&mut self) {
        self.task.abort();
    }
}
