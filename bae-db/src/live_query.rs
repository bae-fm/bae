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
    /// The query's read failed; the next write that reruns it delivers again.
    #[error(transparent)]
    Query(#[from] coven::CovenError),
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
    query: tokio::sync::Mutex<Option<coven::ReconfigurableLiveQuery<Request, Projection>>>,
    cancellation: CancellationToken,
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
    /// `query`, ended by `cancel` or by `cancellation`.
    pub fn new(
        query: coven::ReconfigurableLiveQuery<Request, Projection>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            requests: std::sync::Mutex::new(Some(query.requests())),
            #[cfg(any(test, feature = "test-utils"))]
            runs: std::sync::Mutex::new(query.runs()),
            query: tokio::sync::Mutex::new(Some(query)),
            cancellation,
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
    /// the subscription. Query errors are events, not the end.
    pub async fn next(
        &self,
    ) -> Result<coven::ReconfigurableLiveQueryEvent<Request, Projection>, LiveQueryCancelled> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveQueryCancelled),
            event = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveQueryCancelled)?;
                let event = query.next().await.ok_or(LiveQueryCancelled);
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
    pub(crate) fn cancel(&self) {
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
