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
/// each write that changes what it reads. Made over a token the library's
/// closing cancels, by the library manager or the import service, so closing
/// the library cancels it before the store closes.
pub struct LiveValues<T> {
    query: tokio::sync::Mutex<Option<coven::LiveQuery<T>>>,
    cancellation: CancellationToken,
    /// How often the query had run when it last returned a value, kept here
    /// because a waiting pull holds the query itself.
    #[cfg(any(test, feature = "test-utils"))]
    runs: std::sync::Mutex<coven::LiveQueryRuns>,
}

impl<T: Clone + PartialEq + Send + 'static> LiveValues<T> {
    pub fn new(query: coven::LiveQuery<T>, cancellation: CancellationToken) -> Self {
        Self {
            #[cfg(any(test, feature = "test-utils"))]
            runs: std::sync::Mutex::new(query.runs()),
            query: tokio::sync::Mutex::new(Some(query)),
            cancellation,
        }
    }

    /// The next value, or why there is none.
    pub async fn next(&self) -> Result<T, LiveValuesError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveValuesError::Cancelled),
            value = async {
                let mut query = self.query.lock().await;
                let query = query.as_mut().ok_or(LiveValuesError::Cancelled)?;
                let value = query.next().await;
                #[cfg(any(test, feature = "test-utils"))]
                {
                    *self.runs.lock().expect("live values run count mutex poisoned") =
                        query.runs();
                }
                match value {
                    Some(value) => Ok(value?),
                    None => Err(LiveValuesError::Cancelled),
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
