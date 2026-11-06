use crate::DbError;
use coven::{CovenError, SqlReadContext};
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
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, CovenError>
        + Send
        + 'static,
    R: Send + 'static,
{
    pub(crate) fn new(read: coven::Read<'a, F>, bugs: &'a dyn coven::task::BugSink) -> Self {
        Self { read, bugs }
    }

    pub async fn process<P, T>(self, process: P) -> Result<T, DbError>
    where
        P: FnOnce(R) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        let bugs = self.bugs;
        self.read
            .process(move |rows| process(rows).map_err(CovenError::from))
            .await
            .map_err(|error| DbError::from(coven::task::reported(bugs, error)))
    }
}

impl<'a, F, R> IntoFuture for StoreRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, CovenError>
        + Send
        + 'static,
    R: Send + 'static,
{
    type Output = Result<R, DbError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let bugs = self.bugs;
            self.read.await.map_err(|error| DbError::from(coven::task::reported(bugs, error)))
        })
    }
}
