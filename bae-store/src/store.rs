//! The library's store: bae's handle on coven's, which every read, write and
//! live query of the library runs through.

use crate::contexts::{BulkContext, ReadContext, WriteContext};
use coven::task::{BugSinkRef, CarriesBug};
use coven::{ClockRef, CovenHandle};
#[cfg(any(test, feature = "test-utils"))]
use std::path::Path;
use std::sync::Arc;

mod coven_capabilities;
pub use coven_capabilities::{unsent_to_storage, StorageSetupSteps};
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;

use crate::read_statement::{at_most_one, exactly_one, statement_row, statement_rows};
use crate::store_read::StoreRead;
use crate::DbError;

/// `clock`'s time as a UTC date-time.
pub fn utc_now(clock: &dyn coven::Clock) -> time::UtcDateTime {
    time::UtcDateTime::from(clock.now())
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
    /// How many reads the store has been asked for, so a test can hold a code
    /// path to the reads it should make.
    #[cfg(any(test, feature = "test-utils"))]
    reads: std::sync::atomic::AtomicU64,
    /// The clock and ids a test built the store's library on, which the
    /// library's services open on. Absent on a store a test made over a
    /// handle of its own.
    #[cfg(any(test, feature = "test-utils"))]
    sources: Option<crate::TestSources>,
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
        Store {
            inner: Arc::new(StoreInner {
                handle,
                clock,
                bugs,
                #[cfg(any(test, feature = "test-utils"))]
                reads: std::sync::atomic::AtomicU64::new(0),
                #[cfg(any(test, feature = "test-utils"))]
                sources: None,
            }),
        }
    }

    /// The store over `handle`, built from `sources`, which it keeps.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn new_on(
        handle: CovenHandle,
        sources: crate::TestSources,
        bugs: BugSinkRef,
    ) -> Self {
        Store {
            inner: Arc::new(StoreInner {
                handle,
                clock: sources.clock().clone(),
                bugs,
                reads: std::sync::atomic::AtomicU64::new(0),
                sources: Some(sources),
            }),
        }
    }

    /// The clock and ids this store's library was built on. A test store
    /// over a handle of its own has none; open a library over one made with
    /// `TestSources::store`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn test_sources(&self) -> &crate::TestSources {
        self.inner
            .sources
            .as_ref()
            .expect("a store opened as a test library is made with TestSources::store")
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
    pub fn read<F, R>(
        &self,
        f: F,
    ) -> StoreRead<
        '_,
        impl for<'conn> FnOnce(coven::SqlReadContext<'conn>) -> Result<R, DbError> + Send + 'static,
    >
    where
        F: for<'conn> FnOnce(ReadContext<'conn>) -> Result<R, DbError> + Send + 'static,
        R: Send + 'static,
    {
        #[cfg(any(test, feature = "test-utils"))]
        self.inner.reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        StoreRead::new(
            self.inner.handle.read(move |sql| f(ReadContext::new(sql))),
            self.inner.bugs.as_ref(),
        )
    }

    /// Every row of the generated statement `statement`, read once.
    pub async fn read_rows<Q>(&self, statement: Q) -> Result<Vec<Q::Row>, DbError>
    where
        Q: coven::Read<Handle = sqlive_reads::Writer> + Send + 'static,
        Q::Row: Send + 'static,
    {
        self.read(move |sql| Ok(statement_rows(&sql, &statement)?)).await
    }

    /// The one row of the generated statement `statement`, read once.
    pub async fn read_row<Q>(&self, statement: Q) -> Result<Q::Row, DbError>
    where
        Q: coven::Read<Handle = sqlive_reads::Writer> + Send + 'static,
        Q::Row: Send + 'static,
    {
        self.read(move |sql| statement_row(&sql, &statement)).await
    }

    /// How many reads have been asked of this store since it was made.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn reads_asked(&self) -> u64 {
        self.inner.reads.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Checkpoint the store's WAL into its database file now, for a test of
    /// what follows a checkpoint, such as coven's record of the file's size.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn checkpoint_for_test(&self) {
        self.inner
            .handle
            .checkpoint(coven::CheckpointMode::Truncate)
            .await
            .expect("the store checkpoints its WAL");
    }

    /// Run a write with a borrowed transaction context.
    pub async fn call<R>(
        &self,
        f: impl for<'ctx> FnOnce(&WriteContext<'ctx>) -> Result<R, DbError> + Send + 'static,
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
        f: impl for<'ctx> FnOnce(WriteContext<'ctx>) -> Result<R, DbError> + Send + 'static,
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
        f: impl for<'conn> FnOnce(&BulkContext<'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.reported(self.inner.handle.write_bulk(move |sql| f(&BulkContext { sql: &sql })).await)
    }

    /// A live query of `reads`, statements bae's build compiled: coven runs
    /// them again for each write that touches what they read.
    pub fn subscribe<S: coven::LiveReads>(&self, reads: S) -> coven::LiveQuery<S::Rows, DbError> {
        self.inner.handle.subscribe(reads)
    }

    /// The one row the generated statement `statement` returns, as a live
    /// query; any other number of rows is an error.
    pub fn subscribe_row<Q>(&self, statement: Q) -> coven::LiveQuery<Q::Row, DbError>
    where
        Q: coven::Read<Handle = sqlive_reads::Writer> + Send + Sync + 'static,
        Q::Row: Clone + PartialEq + Send + 'static,
    {
        self.subscribe(statement).process(exactly_one::<Q, _>)
    }

    /// The row the generated statement `statement` returns, for a statement
    /// that returns at most one, as a live query; a second row is an error.
    pub fn subscribe_optional<Q>(&self, statement: Q) -> coven::LiveQuery<Option<Q::Row>, DbError>
    where
        Q: coven::Read<Handle = sqlive_reads::Writer> + Send + Sync + 'static,
        Q::Row: Clone + PartialEq + Send + 'static,
    {
        self.subscribe(statement).process(at_most_one::<Q, _>)
    }

    /// `query`'s results for async code to take, reporting a panic around a
    /// run to the store's bug sink.
    pub fn results<L: crate::live_results::CovenLiveQuery>(
        &self,
        query: L,
    ) -> crate::live_results::LiveResults<L> {
        crate::live_results::LiveResults::new(query, self.inner.bugs.clone())
    }

    /// A live query whose request can be replaced without a new
    /// subscription: `reads` gives the statements bae's build compiled that
    /// each request reads, or the error that fails its run. It fails with
    /// `E`, of which a failed read is one.
    pub fn subscribe_reconfigurable<Q, S, E>(
        &self,
        initial_request: Q,
        reads: impl Fn(&Q) -> Result<S, E> + Send + Sync + 'static,
    ) -> coven::ReconfigurableLiveQuery<Q, S::Rows, E>
    where
        Q: Clone + PartialEq + Send + Sync + 'static,
        S: coven::LiveReads,
        E: From<coven::DbError> + CarriesBug + Send + 'static,
    {
        self.inner.handle.subscribe_reconfigurable(initial_request, reads)
    }

    /// Now, by the clock the store's writes stamp their rows with.
    #[cfg(desktop)]
    pub fn now(&self) -> std::time::SystemTime {
        self.inner.clock.now()
    }

    /// Now, by the clock the store's writes stamp their rows with, as a UTC date-time.
    pub fn now_utc(&self) -> time::UtcDateTime {
        utc_now(self.inner.clock.as_ref())
    }

    /// Run one coven write, its future boxed to keep it off the caller's
    /// async stack, including the import worker's stack. It fails with the
    /// closure's own error, which keeps a failed rollback or byte removal
    /// beside the write's failure: `DbError`, or sync's for a write whose
    /// step is one of coven's sync operations.
    pub fn write<R, E>(
        &self,
        write: impl for<'ctx> FnOnce(WriteContext<'ctx>) -> Result<R, E> + Send + 'static,
    ) -> impl std::future::Future<Output = Result<R, E>> + Send + '_
    where
        R: Send + 'static,
        E: coven::WriteFailure + Send + 'static,
    {
        Box::pin(async move {
            self.reported(self.inner.handle.write(move |sql| write(WriteContext::new(sql))).await)
        })
    }

    /// Stage app bytes, then run a write that puts them in its rows: `stage`
    /// returns what the write needs, such as the ids staging gave the files,
    /// and `sql` gets it once the write has taken the staged files. The
    /// staging future is boxed, as the write's is, to keep coven's buffers off
    /// the caller's async stack.
    pub async fn write_staged<T, R, E>(
        &self,
        stage: impl FnOnce(&mut coven::WriteBatch<'_>) -> Result<T, E> + Send + 'static,
        sql: impl for<'ctx> FnOnce(WriteContext<'ctx>, T) -> Result<R, E> + Send + 'static,
    ) -> Result<R, E>
    where
        T: Send + 'static,
        R: Send + 'static,
        E: coven::WriteFailure + Send + 'static,
    {
        let staged = self.reported(Box::pin(self.inner.handle.stage_files(stage)).await)?;
        self.write(move |write| {
            let value = write.sql.put_staged(staged)?;
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
        Self::new_test_within(schema, directory, clock, bugs, coven::FileBudget::for_test()).await
    }

    /// [`Self::new_test_reporting_to`], opening the person's files within
    /// `files`, a budget a test sizes and watches. The installation and the
    /// store's test sources share it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test_within(
        schema: crate::Schema,
        directory: &Path,
        clock: ClockRef,
        bugs: BugSinkRef,
        files: coven::FileBudget,
    ) -> Result<Self, DbError> {
        let sources = crate::TestSources::on_budget(&clock, files);
        let installation =
            sources.installation(bae_app_dir::AppDir::at(directory)?.coven_app_dir());
        let handle = crate::test_devices::create_test_store(
            sources.builder(&installation),
            schema,
            "Test Library",
            "Test Device",
        )
        .await?;
        Ok(Self::new_on(handle, sources, bugs))
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
