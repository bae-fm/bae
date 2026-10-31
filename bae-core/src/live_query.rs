//! One coven live query, held so it can be reconfigured and cancelled.
//!
//! A subscription hands a UI a stream of values and takes back requests for
//! different ones. Three things make that work: the query, the handle that
//! replaces its request, and a cancellation that refuses every later request
//! and settles the read a caller is already waiting on. Every subscription a
//! surface pulls values from — the browse lists, the searches, the import
//! list — owns exactly this, and owns it through here.
//!
//! The cancellation is the library's too: one is made only by
//! [`LibraryManager::until_closed`](crate::library::LibraryManager::until_closed),
//! and closing the library cancels it before the store closes, so no
//! subscription answers with the closed store.

use tokio_util::sync::CancellationToken;

/// The subscription is over: it was cancelled, or its query is gone or has
/// ended.
pub(crate) struct LiveQueryCancelled;

/// A reconfigurable live query and the cancellation that ends it.
pub(crate) struct CancellableLiveQuery<Request, Projection> {
    requests: std::sync::Mutex<Option<coven::LiveQueryRequests<Request>>>,
    query: tokio::sync::Mutex<Option<coven::ReconfigurableLiveQuery<Request, Projection>>>,
    cancellation: CancellationToken,
    /// The query's run count, held here because a waiting read holds the
    /// query itself.
    #[cfg(feature = "test-utils")]
    runs: coven::LiveQueryRuns,
}

impl<Request, Projection> CancellableLiveQuery<Request, Projection>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Projection: Clone + PartialEq + Send + 'static,
{
    /// `query`, ended by [`Self::cancel`] or by `cancellation`.
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<Request, Projection>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            requests: std::sync::Mutex::new(Some(query.requests())),
            #[cfg(feature = "test-utils")]
            runs: query.runs(),
            query: tokio::sync::Mutex::new(Some(query)),
            cancellation,
        }
    }

    /// A handle that counts the query's runs.
    #[cfg(feature = "test-utils")]
    pub(crate) fn runs(&self) -> coven::LiveQueryRuns {
        self.runs.clone()
    }

    /// Evaluate a new absolute request and return the revision that will
    /// deliver it. Repeating the standing request keeps its revision.
    #[expect(
        clippy::map_err_ignore,
        reason = "coven refuses a request only because its query is gone, which is this cancellation"
    )]
    pub(crate) fn set(&self, request: Request) -> Result<u64, LiveQueryCancelled> {
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
    pub(crate) async fn next(
        &self,
    ) -> Result<coven::ReconfigurableLiveQueryEvent<Request, Projection>, LiveQueryCancelled> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveQueryCancelled),
            event = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveQueryCancelled)?;
                query.next().await.ok_or(LiveQueryCancelled)
            } => event,
        }
    }
}

impl<Request, Projection> CancellableLiveQuery<Request, Projection> {
    /// Refuse every later request and settle a waiting read. The query itself
    /// lives until [`Self::close`] takes it, or until this is dropped.
    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
        self.requests
            .lock()
            .expect("live query request mutex poisoned")
            .take();
    }

    /// Cancel, then drop the query once no read holds it.
    pub(crate) async fn close(&self) {
        self.cancel();
        self.query.lock().await.take();
    }
}

impl<Request, Projection> Drop for CancellableLiveQuery<Request, Projection> {
    fn drop(&mut self) {
        self.cancel();
    }
}
