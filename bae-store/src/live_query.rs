//! One coven live query, held so it can be reconfigured and cancelled, and
//! [`LiveValues`], one a surface only pulls values from.
//!
//! A subscription hands a UI a stream of values and takes back requests for
//! different ones. Three things make that work: the query, the handle that
//! replaces its request, and a cancellation that refuses every later request
//! and settles the read a caller is already waiting on. Every subscription a
//! surface pulls values from — the browse lists, the searches, the import
//! list — owns exactly this, and owns it through here.
//!
//! The cancellation is the library's too: one is made only by the library
//! manager's `until_closed`, and closing the library cancels it before the
//! store closes, so no subscription answers with the closed store.
//!
//! A query whose run panics answers once with the bug, then ends. Whoever
//! awaits a coven query reports such a bug, and every subscription a surface
//! pulls is awaited here, so each takes the bug sink it reports to.

use tokio_util::sync::CancellationToken;

mod values;
pub use values::{LiveSource, LiveValues};

/// The subscription is over: it was cancelled, or its query is gone or has
/// ended.
pub struct LiveQueryCancelled;

/// Why a subscription's read or request got no answer.
#[derive(Debug, thiserror::Error)]
pub enum LiveQueryError {
    /// The surface cancelled, or the library closed.
    #[error("live query cancelled")]
    Cancelled,
    /// The query's read failed. The next write that reruns it delivers
    /// again, unless the failure holds a bug: a run that panicked, reported
    /// as it arrived, or an earlier bug the store failed with. The query has
    /// ended then.
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

impl bae_util::task_failure::ReportedBug for LiveQueryError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Cancelled => None,
            Self::Query(error) => error.reported_bug(),
        }
    }
}

/// The live query ending is the subscription's cancellation.
impl From<LiveQueryCancelled> for LiveQueryError {
    fn from(_: LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// A reconfigurable live query and the cancellation that ends it.
pub struct CancellableLiveQuery<Request, Projection> {
    requests: std::sync::Mutex<Option<coven::LiveQueryRequests<Request>>>,
    query: tokio::sync::Mutex<
        Option<coven::ReconfigurableLiveQuery<Request, Projection, coven::CovenError>>,
    >,
    cancellation: CancellationToken,
    /// Where a bug a run of the query carries goes as it arrives.
    bugs: coven::task::BugSinkRef,
    /// How often the query had run when it last returned an event, kept
    /// here because a waiting read holds the query itself.
    #[cfg(any(test, feature = "test-utils"))]
    runs: std::sync::Mutex<coven::LiveQueryRuns>,
}

impl<Request, Projection> CancellableLiveQuery<Request, Projection>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Projection: Clone + PartialEq + Send + 'static,
{
    /// `query`, ended by `cancel` or by `cancellation`, reporting a bug a
    /// run carries to `bugs`.
    pub fn new(
        query: coven::ReconfigurableLiveQuery<Request, Projection, coven::CovenError>,
        cancellation: CancellationToken,
        bugs: coven::task::BugSinkRef,
    ) -> Self {
        Self {
            requests: std::sync::Mutex::new(Some(query.requests())),
            #[cfg(any(test, feature = "test-utils"))]
            runs: std::sync::Mutex::new(query.runs()),
            query: tokio::sync::Mutex::new(Some(query)),
            cancellation,
            bugs,
        }
    }

    /// How often the query had run when it last returned an event, for a
    /// test that asks how often a write reran it: a run whose result was
    /// unchanged counts once a later event returns.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn runs(&self) -> coven::LiveQueryRuns {
        *self.runs.lock().expect("live query run count mutex poisoned")
    }

    /// Evaluate a new absolute request and return the revision that will
    /// deliver it. Repeating the standing request keeps its revision.
    #[expect(
        clippy::map_err_ignore,
        reason = "coven refuses a request only because its query is gone, which is this cancellation"
    )]
    pub fn set(&self, request: Request) -> Result<u64, LiveQueryCancelled> {
        if self.cancellation.is_cancelled() {
            return Err(LiveQueryCancelled);
        }
        self.requests
            .lock()
            .expect("live query request mutex poisoned")
            .as_ref()
            .ok_or(LiveQueryCancelled)?
            .set(request)
            .map(|revision| revision.0)
            .map_err(|_| LiveQueryCancelled)
    }

    /// The next value, its revision, and why it was produced — or the end of
    /// the subscription. Query errors are events, not the end; one that
    /// carries a bug is reported as it arrives.
    pub async fn next(
        &self,
    ) -> Result<
        coven::ReconfigurableLiveQueryEvent<Request, Projection, coven::CovenError>,
        LiveQueryCancelled,
    > {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveQueryCancelled),
            event = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveQueryCancelled)?;
                let event = query.next().await.ok_or(LiveQueryCancelled);
                if let Ok(coven::ReconfigurableLiveQueryEvent { result: Err(error), .. }) = &event {
                    coven::task::report_carried(self.bugs.as_ref(), error);
                }
                #[cfg(any(test, feature = "test-utils"))]
                {
                    *self.runs.lock().expect("live query run count mutex poisoned") =
                        query.runs();
                }
                event
            } => event,
        }
    }
}

impl<Request, Projection> CancellableLiveQuery<Request, Projection> {
    /// Refuse every later request and settle a waiting read. The query itself
    /// lives until [`Self::close`] takes it, or until this is dropped.
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.requests.lock().expect("live query request mutex poisoned").take();
    }

    /// Cancel, then drop the query once no read holds it.
    pub async fn close(&self) {
        self.cancel();
        self.query.lock().await.take();
    }
}

impl<Request, Projection> Drop for CancellableLiveQuery<Request, Projection> {
    fn drop(&mut self) {
        self.cancel();
    }
}
