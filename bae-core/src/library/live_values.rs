//! One coven live query a surface pulls each value of, with nothing to
//! reconfigure, ended by the surface or by the library closing.

use tokio_util::sync::CancellationToken;

/// Why a pull got no value.
#[derive(Debug, thiserror::Error)]
pub enum LiveValuesError {
    /// The surface cancelled, or the library closed.
    #[error("live values cancelled")]
    Cancelled,
    /// The query's read failed; the next write that reruns it delivers again.
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// A live query's values, one per pull: the first at once, then one after
/// each write that changes what it reads. Made only by the library manager,
/// over the library's closing, so closing the library cancels it before the
/// store closes.
pub struct LiveValues<T> {
    query: tokio::sync::Mutex<Option<coven::LiveQuery<T>>>,
    cancellation: CancellationToken,
}

impl<T: Clone + PartialEq + Send + 'static> LiveValues<T> {
    pub(crate) fn new(query: coven::LiveQuery<T>, cancellation: CancellationToken) -> Self {
        Self { query: tokio::sync::Mutex::new(Some(query)), cancellation }
    }

    /// The next value, or why there is none.
    pub async fn next(&self) -> Result<T, LiveValuesError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveValuesError::Cancelled),
            value = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveValuesError::Cancelled)?;
                match query.next().await {
                    Some(value) => Ok(value?),
                    None => Err(LiveValuesError::Cancelled),
                }
            } => value,
        }
    }

    /// A handle that counts the query's runs, for a test that asks how often
    /// a write reran it. Asked before any pull waits.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn runs(&self) -> coven::LiveQueryRuns {
        self.query
            .try_lock()
            .expect("no pull waits while a test counts the runs")
            .as_ref()
            .expect("the values are not cancelled")
            .runs()
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
