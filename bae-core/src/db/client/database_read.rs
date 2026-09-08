use super::{CovenError, Database, DbError, SqlReadContext};
use std::future::{Future, IntoFuture};
use std::pin::Pin;

/// A Coven read with the database client's error type at the calling boundary.
#[must_use = "reads do not execute until awaited"]
pub(super) struct DatabaseRead<'a, F> {
    read: coven::Read<'a, F>,
}

impl<'a, F, R> DatabaseRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, CovenError>
        + Send
        + 'static,
    R: Send + 'static,
{
    pub(super) fn new(read: coven::Read<'a, F>) -> Self {
        Self { read }
    }

    pub(super) async fn process<P, T>(self, process: P) -> Result<T, DbError>
    where
        P: FnOnce(R) -> Result<T, DbError> + Send + 'static,
        T: Send + 'static,
    {
        self.read
            .process(move |rows| process(rows).map_err(CovenError::from))
            .await
            .map_err(Database::coven_error)
    }
}

impl<'a, F, R> IntoFuture for DatabaseRead<'a, F>
where
    F: for<'connection> FnOnce(SqlReadContext<'connection>) -> Result<R, CovenError>
        + Send
        + 'static,
    R: Send + 'static,
{
    type Output = Result<R, DbError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.read.await.map_err(Database::coven_error) })
    }
}
