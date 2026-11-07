//! The library's store: bae's handle on coven's, which every read, write and
//! live query of the library runs through.

use coven::task::{BugSinkRef, CarriesBug};
use coven::{ClockRef, CovenHandle, CovenWrite, SqlReadContext};
#[cfg(any(test, feature = "test-utils"))]
use std::path::Path;
use std::sync::Arc;

mod coven_capabilities;
pub use coven_capabilities::{syncs_on, StorageSetupSteps, UPLOADS_PAUSED};
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;

use crate::store_read::StoreRead;
use crate::DbError;

/// `clock`'s time as a UTC date-time.
pub fn utc_now(clock: &dyn coven::Clock) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(clock.now())
}

struct StoreInner {
    /// The top-level coven handle owns the connection and exposes the host SQL
    /// context for each read or write.
    handle: CovenHandle,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    clock: ClockRef,
    /// Where a bug a coven call carries back goes as it arrives: the store
    /// awaits every call it makes on its handle, so it is the one that
    /// reports such a bug, once.
    bugs: BugSinkRef,
}

/// The library's store over coven's owned connection. Writes to synced tables
/// are captured by coven's attached session for changeset sync.
///
/// A coven call that fails on a bug of its own work, a panic in coven or in
/// bae's closure, carries the bug back (`CarriesBug`). The store reports it
/// to its bug sink as the call returns, so every caller above shows the
/// failure and none reports it again (`ReportedBug`). A live query's bug is
/// reported by whoever awaits the query.
///
/// coven also owns connection pragmas such as `foreign_keys` and `journal_mode`:
/// bae never opens a production SQLite connection or sets a production connection
/// pragma, it inherits those guarantees from the coven handle.
#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    /// The store over `handle`, stamping its writes by `clock`, reporting a
    /// bug a call carries back to `bugs`, the app's crash reporter.
    pub fn new(handle: CovenHandle, clock: ClockRef, bugs: BugSinkRef) -> Self {
        Store { inner: Arc::new(StoreInner { handle, clock, bugs }) }
    }

    /// `result`, once the bug its coven call carried back, if any, has gone
    /// to the store's bug sink.
    fn reported<T, E: CarriesBug>(&self, result: Result<T, E>) -> Result<T, E> {
        result.map_err(|error| coven::task::reported(self.inner.bugs.as_ref(), error))
    }

    // Reads release their connection before optional result processing. Every
    // write commits before its future resolves; an error rolls back the write.

    /// Run a pure read on Coven's bounded readers. Attach `.process()` for
    /// result assembly after releasing the connection.
    pub fn read<F, R>(&self, f: F) -> StoreRead<'_, F>
    where
        F: for<'conn> FnOnce(SqlReadContext<'conn>) -> Result<R, DbError> + Send + 'static,
        R: Send + 'static,
    {
        StoreRead::new(self.inner.handle.read(f), self.inner.bugs.as_ref())
    }

    /// Run a write with a borrowed transaction context.
    pub async fn call<R>(
        &self,
        f: impl for<'ctx> FnOnce(&CovenWrite<'ctx>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.call_sql(move |sql| f(&sql)).await
    }

    /// Run an atomic write through coven: the closure's SQL and coven's
    /// steps in one transaction.
    pub async fn call_sql<R>(
        &self,
        f: impl for<'ctx> FnOnce(CovenWrite<'ctx>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.write(f).await
    }

    /// Run a bulk write: SQL that changes device-local tables alone, run
    /// without coven's pre-update hook, so SQLite empties a table in one step
    /// and fills one at its own speed. After it commits, every live query
    /// reading a table it changed runs again once, whichever rows it read.
    pub async fn call_bulk<R>(
        &self,
        f: impl for<'conn> FnOnce(&coven::BulkSqlContext<'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.reported(self.inner.handle.write_bulk(move |sql| f(&sql)).await)
    }

    /// A live query: coven reruns `query` for each write that touches what it
    /// read.
    pub fn subscribe<F, R>(&self, query: F) -> coven::LiveQuery<R, DbError>
    where
        F: Fn(SqlReadContext<'_>) -> Result<R, DbError> + Send + Sync + 'static,
        R: Send + 'static,
    {
        self.inner.handle.subscribe(query)
    }

    /// A live query whose request can be replaced without a new
    /// subscription. It fails with its query's own error, of which a failed
    /// read is one.
    pub fn subscribe_reconfigurable<Q, F, R, E>(
        &self,
        initial_request: Q,
        query: F,
    ) -> coven::ReconfigurableLiveQuery<Q, R, E>
    where
        Q: Clone + PartialEq + Send + Sync + 'static,
        F: Fn(&Q, SqlReadContext<'_>) -> Result<R, E> + Send + Sync + 'static,
        R: Send + 'static,
        E: From<coven::DbError> + CarriesBug + Send + 'static,
    {
        self.inner.handle.subscribe_reconfigurable(initial_request, query)
    }

    /// Now, by the clock the store's writes stamp their rows with.
    #[cfg(desktop)]
    pub fn now(&self) -> std::time::SystemTime {
        self.inner.clock.now()
    }

    /// Now, by the clock the store's writes stamp their rows with, as a UTC date-time.
    pub fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        utc_now(self.inner.clock.as_ref())
    }

    /// Run one coven write, its future boxed to keep it off the caller's
    /// async stack, including the import worker's stack. It fails with the
    /// closure's own error, which keeps a failed rollback or byte removal
    /// beside the write's failure: `DbError`, or sync's for a write whose
    /// step is one of coven's sync operations.
    pub fn write<R, E>(
        &self,
        write: impl for<'ctx> FnOnce(CovenWrite<'ctx>) -> Result<R, E> + Send + 'static,
    ) -> impl std::future::Future<Output = Result<R, E>> + Send + '_
    where
        R: Send + 'static,
        E: coven::WriteFailure + Send + 'static,
    {
        Box::pin(async move { self.reported(self.inner.handle.write(write).await) })
    }

    /// Stage app bytes, then run a write that puts them in its rows: `stage`
    /// returns what the write needs, such as the ids staging gave the files,
    /// and `sql` gets it once the write has taken the staged files. The
    /// staging future is boxed, as the write's is, to keep coven's buffers off
    /// the caller's async stack.
    pub async fn write_staged<T, R, E>(
        &self,
        stage: impl FnOnce(&mut coven::WriteBatch<'_>) -> Result<T, E> + Send + 'static,
        sql: impl for<'ctx> FnOnce(CovenWrite<'ctx>, T) -> Result<R, E> + Send + 'static,
    ) -> Result<R, E>
    where
        T: Send + 'static,
        R: Send + 'static,
        E: coven::WriteFailure + Send + 'static,
    {
        let staged = self.reported(Box::pin(self.inner.handle.stage_files(stage)).await)?;
        self.write(move |write| {
            let value = write.put_staged(staged)?;
            sql(write, value)
        })
        .await
    }

    /// Create an isolated installation on `schema` in `directory`, with
    /// bae's declarations. A bug one of its calls carries back fails the
    /// test as the store drops.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test(
        schema: crate::Schema,
        directory: &Path,
        clock: ClockRef,
    ) -> Result<Self, DbError> {
        Self::new_test_reporting_to(schema, directory, clock, coven::task::HeardBugs::shared())
            .await
    }

    /// [`Self::new_test`], reporting each bug a call carries back to `bugs`,
    /// where a test of a bug takes it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_reporting_to(
        schema: crate::Schema,
        directory: &Path,
        clock: ClockRef,
        bugs: BugSinkRef,
    ) -> Result<Self, DbError> {
        let installation =
            coven::Coven::in_memory(bae_app_dir::AppDir::at(directory)?.coven_app_dir());
        let handle = crate::configure_store(installation.builder(), schema)
            .clock(clock.clone())
            .create_store("Test Library")
            .await?;
        Ok(Self::new(handle, clock, bugs))
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
