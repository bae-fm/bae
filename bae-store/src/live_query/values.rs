//! One live source of values a surface pulls each value of, with nothing
//! to reconfigure, ended by the surface or by the library closing.

use super::LiveQueryError;
use tokio_util::sync::CancellationToken;

/// The future of a source's next value: a value, a failed read, or `None`
/// once the source has ended with its store.
pub type NextValue<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<Result<T, crate::DbError>>> + Send + 'a>,
>;

/// What a surface pulls values from: a coven live query, or coven's sync
/// state beside one ([`crate::WithSyncState`]).
pub trait LiveSource<T>: Send {
    /// The next value, a failed read, or `None` once the source has ended
    /// with its store.
    fn next(&mut self) -> NextValue<'_, T>;

    /// How often the source's query had run when it last returned a value.
    #[cfg(any(test, feature = "test-utils"))]
    fn runs(&self) -> coven::LiveQueryRuns;
}

/// A coven live query, failing with bae's own error or coven's.
impl<T, E> LiveSource<T> for coven::LiveQuery<T, E>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<crate::DbError> + Send + 'static,
{
    fn next(&mut self) -> NextValue<'_, T> {
        Box::pin(async move { Some(coven::LiveQuery::next(self).await?.map_err(Into::into)) })
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
    fn next(&mut self) -> NextValue<'_, T> {
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
    /// Where a bug a failed read carries goes as it arrives.
    bugs: coven::task::BugSinkRef,
    /// How often the query had run when it last returned a value, kept here
    /// because a waiting pull holds the query itself.
    #[cfg(any(test, feature = "test-utils"))]
    runs: std::sync::Mutex<coven::LiveQueryRuns>,
}

impl<T: Clone + PartialEq + Send + 'static> LiveValues<T> {
    /// `query`'s values, ended by `cancellation`, reporting a bug a failed
    /// read carries to `bugs`.
    pub fn new(
        query: impl LiveSource<T> + 'static,
        cancellation: CancellationToken,
        bugs: coven::task::BugSinkRef,
    ) -> Self {
        Self {
            #[cfg(any(test, feature = "test-utils"))]
            runs: std::sync::Mutex::new(query.runs()),
            query: tokio::sync::Mutex::new(Some(Box::new(query))),
            cancellation,
            bugs,
        }
    }

    /// The next value, or why there is none. A failed read that carries a
    /// bug is reported as it arrives.
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
                    Some(value) => {
                        Ok(value.map_err(|error| coven::task::reported(self.bugs.as_ref(), error))?)
                    }
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

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
