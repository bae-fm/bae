use crate::DbError;
use coven::SqlReadContext;
use std::future::{Future, IntoFuture};
use std::pin::Pin;

/// A Coven read with the store's error type at the calling boundary, its
/// carried bug reported as [`crate::Store`] reports every call's.
#[must_use = "reads do not execute until awaited"]
pub struct StoreRead<'a, F> {
    read: coven::Read<'a, F>,
    /// The store's bug sink, which hears a bug the read carries back.
    bugs: &'a dyn coven::task::BugSink,
}

impl<'a, F, R> StoreRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, DbError> + Send + 'static,
    R: Send + 'static,
{
    pub(crate) fn new(read: coven::Read<'a, F>, bugs: &'a dyn coven::task::BugSink) -> Self {
        Self { read, bugs }
    }

    /// Process what the read returned on another worker, once its
    /// connection is released. The processing fails with its own error, of
    /// which a failed read is one.
    pub async fn process<P, T, E>(self, process: P) -> Result<T, E>
    where
        P: FnOnce(R) -> Result<T, E> + Send + 'static,
        T: Send + 'static,
        E: From<DbError> + Send + 'static,
    {
        let bugs = self.bugs;
        match self.read.process(move |rows| Ok(process(rows))).await {
            Ok(processed) => processed,
            Err(error) => Err(coven::task::reported(bugs, error).into()),
        }
    }
}

impl<'a, F, R> IntoFuture for StoreRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, DbError> + Send + 'static,
    R: Send + 'static,
{
    type Output = Result<R, DbError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let bugs = self.bugs;
            self.read.await.map_err(|error| coven::task::reported(bugs, error))
        })
    }
}
