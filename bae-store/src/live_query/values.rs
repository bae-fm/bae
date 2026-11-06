//! One live source of values a surface pulls each value of, with nothing
//! to reconfigure, ended by the surface or by the library closing.

use super::LiveQueryError;
use tokio_util::sync::CancellationToken;

/// What a surface pulls values from: a coven live query, or coven's sync
/// state beside one ([`crate::WithSyncState`]).
pub trait LiveSource<T>: Send {
    /// The next value, a failed read, or `None` once the source has ended
    /// with its store.
    fn next(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<coven::CovenResult<T>>> + Send + '_>,
    >;

    /// How often the source's query had run when it last returned a value.
    #[cfg(any(test, feature = "test-utils"))]
    fn runs(&self) -> coven::LiveQueryRuns;
}

impl<T: Clone + PartialEq + Send + 'static> LiveSource<T>
    for coven::LiveQuery<T, coven::CovenError>
{
    fn next(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<coven::CovenResult<T>>> + Send + '_>,
    > {
        Box::pin(coven::LiveQuery::next(self))
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn runs(&self) -> coven::LiveQueryRuns {
        coven::LiveQuery::runs(self)
    }
}

impl<R, T> LiveSource<T> for crate::WithSyncState<R, T>
where
    R: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    fn next(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Option<coven::CovenResult<T>>> + Send + '_>,
    > {
        Box::pin(crate::WithSyncState::next(self))
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn runs(&self) -> coven::LiveQueryRuns {
        crate::WithSyncState::runs(self)
    }
}

/// A live source's values, one per pull: the first at once, then one after
/// each change to what it reads. Made over a token the library's
/// closing cancels, by the library manager or the import service, so closing
/// the library cancels it before the store closes.
pub struct LiveValues<T> {
    query: tokio::sync::Mutex<Option<Box<dyn LiveSource<T>>>>,
    cancellation: CancellationToken,
    /// How often the query had run when it last returned a value, kept here
    /// because a waiting pull holds the query itself.
    #[cfg(any(test, feature = "test-utils"))]
    runs: std::sync::Mutex<coven::LiveQueryRuns>,
}

impl<T: Clone + PartialEq + Send + 'static> LiveValues<T> {
    pub fn new(query: impl LiveSource<T> + 'static, cancellation: CancellationToken) -> Self {
        Self {
            #[cfg(any(test, feature = "test-utils"))]
            runs: std::sync::Mutex::new(query.runs()),
            query: tokio::sync::Mutex::new(Some(Box::new(query))),
            cancellation,
        }
    }

    /// The next value, or why there is none.
    pub async fn next(&self) -> Result<T, LiveQueryError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveQueryError::Cancelled),
            value = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveQueryError::Cancelled)?;
                let value = query.next().await;
                #[cfg(any(test, feature = "test-utils"))]
                {
                    *self.runs.lock().expect("live values run count mutex poisoned") =
                        query.runs();
                }
                match value {
                    Some(value) => Ok(value?),
                    None => Err(LiveQueryError::Cancelled),
                }
            } => value,
        }
    }

    /// How often the query had run when it last returned a value, for a
    /// test that asks how often a write reran it: a run whose result was
    /// unchanged counts once a later value returns.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn runs(&self) -> coven::LiveQueryRuns {
        *self.runs.lock().expect("live values run count mutex poisoned")
    }

    /// Settle a waiting pull and refuse later ones, then drop the query once
    /// no pull holds it.
    pub async fn cancel(&self) {
        self.cancellation.cancel();
        self.query.lock().await.take();
    }
}

impl<T> Drop for LiveValues<T> {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
