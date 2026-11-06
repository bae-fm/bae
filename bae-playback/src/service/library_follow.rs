//! A live library read the run loop keeps pointed at what the service is
//! playing, and wakes on when the library changes what it reads.
//!
//! The loop hands it its request every turn, like the side-pause countdown
//! wait: an unchanged request keeps the query, a new one re-points it. The
//! query runs on a task of its own, so the loop's `select!` dropping the
//! branch never throws away a read in flight; the loop's only branch is a
//! channel receive.

use tracing::{debug, warn};

pub(super) struct LibraryFollow<Request, Value> {
    requests: coven::LiveQueryRequests<Request>,
    request: Request,
    reads: tokio::sync::mpsc::UnboundedReceiver<(Request, Value)>,
    task: bae_diagnostics::ReportedTask,
}

impl<Request, Value> LibraryFollow<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + std::fmt::Debug + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    /// Follow `query` on a task of its own. A read that fails is reported
    /// through `diagnostics` under `what`, as `failed_read` says: playback
    /// learns that a track it holds was deleted only from these reads. While
    /// the store is open the follow keeps running, and the next change reads
    /// again. A failure because the store closed or because its handle failed
    /// ends the query, and the follow reads nothing more. A follow that panics
    /// is reported through `diagnostics` and reads nothing more either.
    pub(super) fn new(
        mut query: coven::ReconfigurableLiveQuery<Request, Value, coven::CovenError>,
        request: Request,
        what: &'static str,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Self {
        let requests = query.requests();
        let (tx, reads) = tokio::sync::mpsc::unbounded_channel();
        let bugs = diagnostics.clone();
        let operation = format!("read {what} for playback");
        let task = diagnostics.spawn_reported(
            &tokio::runtime::Handle::current(),
            "follow a library read for playback",
            async move {
                while let Some(event) = query.next().await {
                    let request = event.request.clone();
                    match event.result {
                        Ok(value) => {
                            if tx.send((request, value)).is_err() {
                                return;
                            }
                        }
                        Err(error) => failed_read(&bugs, &operation, what, &request, &error),
                    }
                }
            },
            // Its sender went with the task, so `next` waits as it does for a
            // closed store: the service plays on without the follow.
            |_| {},
        );
        Self { requests, request, reads, task }
    }

    /// Point the read at `request`, unless it already reads it. A follow
    /// whose query ended, once its store closed or its handle failed, reads
    /// nothing more, for this request or any later one.
    pub(super) fn follow(&mut self, request: Request) {
        if request != self.request {
            if self.requests.set(request.clone()).is_err() {
                warn!("the follow's query has ended; {request:?} is not read");
            }
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

/// Say that the read for `request` of `what` failed with `error`. A failed
/// read is a bug, reported under `operation`, but for two failures that are
/// not this read's: the store closing, which ends the follow and playback
/// with it, and the store failing on an earlier bug, which coven reported as
/// it happened and now repeats to every call as `StoreFailed`. A run that
/// panicked carries its bug, which is reported as the panic it is.
fn failed_read(
    bugs: &bae_diagnostics::Diagnostics,
    operation: &str,
    what: &str,
    request: &impl std::fmt::Debug,
    error: &coven::CovenError,
) {
    match error {
        coven::CovenError::Database(coven::DbError::StoreClosed) => {
            debug!("the store closed, so {what} is read no more");
        }
        coven::CovenError::Database(coven::DbError::StoreFailed(bug)) => {
            warn!("the store failed on a bug, so {what} is read no more: {bug}");
        }
        error => match coven::task::CarriesBug::bug(error) {
            Some(bug) => {
                bugs.panicked(bug.clone());
            }
            None => {
                bugs.report_bug(operation, &format_args!("reading for {request:?} failed: {error}"))
            }
        },
    }
}

impl<Request, Value> Drop for LibraryFollow<Request, Value> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
#[path = "library_follow_tests.rs"]
mod tests;
