//! One live source of values a surface pulls each value of, with nothing
//! to reconfigure, ended by the surface or by the library closing.

use super::LiveQueryError;
use crate::live_results::LiveResults;
use tokio_util::sync::CancellationToken;

/// The future of a source's next value: a value, a failed read, or `None`
/// once the source has ended with its store.
pub type NextValue<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<Result<T, crate::DbError>>> + Send + 'a>,
>;

/// What a surface pulls values from: a coven live query.
pub trait LiveSource<T>: Send {
    /// The next value, a failed read, or `None` once the source has ended
    /// with its store.
    fn next(&mut self) -> NextValue<'_, T>;
}

/// A coven live query's results, failing with bae's own error or coven's.
impl<T, E> LiveSource<T> for LiveResults<coven::LiveQuery<T, E>>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<crate::DbError> + Send + 'static,
{
    fn next(&mut self) -> NextValue<'_, T> {
        Box::pin(async move { Some(LiveResults::next(self).await?.map_err(Into::into)) })
    }
}

/// What [`LiveValues`] pulls values from: a coven live query, whose results
/// it takes through [`LiveResults`].
pub trait IntoLiveSource<T> {
    /// The source, reporting a panic around a coven run to `bugs`.
    fn into_live_source(self, bugs: coven::task::BugSinkRef) -> Box<dyn LiveSource<T>>;
}

impl<T, E> IntoLiveSource<T> for coven::LiveQuery<T, E>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<crate::DbError> + Send + 'static,
{
    fn into_live_source(self, bugs: coven::task::BugSinkRef) -> Box<dyn LiveSource<T>> {
        Box::new(LiveResults::new(self, bugs))
    }
}

/// A coven live query's results that the store already took through
/// [`LiveResults`], which holds the bug sink its panics go to.
impl<T, E> IntoLiveSource<T> for LiveResults<coven::LiveQuery<T, E>>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Into<crate::DbError> + Send + 'static,
{
    fn into_live_source(self, _bugs: coven::task::BugSinkRef) -> Box<dyn LiveSource<T>> {
        Box::new(self)
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
}

impl<T: Clone + PartialEq + Send + 'static> LiveValues<T> {
    /// `query`'s values, ended by `cancellation`, reporting a bug a failed
    /// read carries to `bugs`.
    pub fn new(
        query: impl IntoLiveSource<T>,
        cancellation: CancellationToken,
        bugs: coven::task::BugSinkRef,
    ) -> Self {
        let query = query.into_live_source(bugs.clone());
        Self { query: tokio::sync::Mutex::new(Some(query)), cancellation, bugs }
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
                match query.next().await {
                    Some(value) => {
                        value.map_err(|error| {
                            LiveQueryError::Query(coven::task::reported(self.bugs.as_ref(), error))
                        })
                    }
                    None => Err(LiveQueryError::Cancelled),
                }
            } => value,
        }
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
