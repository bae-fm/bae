use crate::DbError;
use coven::{CovenError, SqlReadContext};
use std::future::{Future, IntoFuture};
use std::pin::Pin;

/// A Coven read with the store's error type at the calling boundary.
#[must_use = "reads do not execute until awaited"]
pub struct StoreRead<'a, F> {
    read: coven::Read<'a, F>,
}

impl<'a, F, R> StoreRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, CovenError>
        + Send
        + 'static,
    R: Send + 'static,
{
    pub(crate) fn new(read: coven::Read<'a, F>) -> Self {
        Self { read }
    }

    pub async fn process<P, T>(self, process: P) -> Result<T, DbError>
    where
        P: FnOnce(R) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        self.read
            .process(move |rows| process(rows).map_err(CovenError::from))
            .await
            .map_err(DbError::from)
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
        Box::pin(async move { self.read.await.map_err(DbError::from) })
    }
}
