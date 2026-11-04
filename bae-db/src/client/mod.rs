use crate::credits::LibraryCredit;
use crate::models::*;
use bae_util::content_type::ContentType;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Params, Row};
// coven owns production connections; row-mapping tests also use bare SQLite.
use crate::DbError;
use coven::{ClockRef, CovenError, CovenHandle, CovenWrite, SqlReadContext};
#[cfg(any(test, feature = "test-utils"))]
use rusqlite::Connection;
use std::collections::{BTreeSet, HashMap, HashSet};
// Only the test-only external-ref helper names a path type here; production
// paths live on the types that carry them.
#[cfg(any(test, feature = "test-utils"))]
use std::path::Path;
use std::sync::Arc;

/// The columns of a release file row `f` that `row_to_file` reads: its own,
/// and from coven's record of the file it references, `file_size`, the
/// file's size, and `stored`, whether the file is stored. A macro so a
/// caller's `concat!` can build its statement from it.
#[macro_export]
macro_rules! release_file_columns {
    () => {
        "f.*, (SELECT coven.size FROM coven_files AS coven WHERE coven.id = f.file) \
         AS file_size, \
         (SELECT coven.hash IS NOT NULL FROM coven_files AS coven WHERE coven.id = f.file) \
         AS stored"
    };
}

mod album;
mod artist;
pub mod artist_credit_names;
use artist_credit_names::settle_credit_name;
pub mod artist_resolution;
pub mod label_resolution;
#[cfg(any(test, feature = "test-utils"))]
pub use artist_resolution::resolved_in_an_empty_library;
pub use artist_resolution::ArtistCredits;
pub use artist_resolution::IdentityWriteError;
use artist_resolution::{relink_album_artists, relink_track_artists};
use label_resolution::label_write;
pub use label_resolution::LabelWrite;
// Only the import write resolves credits inside a release's own write.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use artist_resolution::{identity_write_failure, relink};
mod file_row;
pub mod identity_merges;
pub use file_row::FileRow;
pub mod pending;
pub mod queued_uploads;
pub mod sync_start_failure;
pub mod sync_status;
pub use pending::PendingRow;
mod coven_capabilities;
pub use coven_capabilities::StorageSetupSteps;
mod database_read;
pub mod files;
pub mod records;
// Only identifying a release, a desktop operation, changes its records.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use records::RecordsChange;
mod admin_operations;
mod collections;
mod history;
mod membership;
mod people;
mod profile_request;
mod release;
#[cfg(desktop)]
pub use release::release_edit_tracks_on;
pub use release::ReleaseMetadataRows;
// Only saving a release edit session writes metadata alongside records.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use release::write_release_metadata_on;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod release_edit_save;
pub use release::AlbumFields;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use release_edit_save::ReleaseEditWrite;
mod release_deletion;
mod release_detail;
pub use release_detail::{release_detail_rows_on, unstored_audio_in_row, DbReleaseDetailRows};
mod release_file;
mod release_locality;
pub use release_deletion::ReleaseDeletion;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
mod seeded_albums;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub use seeded_albums::{SeededAlbum, SeededAudio};
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;
mod track;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

pub mod list_windows;
pub mod path_column;
pub mod pressing_columns;
pub mod query;
pub mod read;
mod scrub_points_column;
pub mod write;

use query::*;
use read::*;
use write::*;

/// `clock`'s time as a UTC date-time.
pub fn utc_now(clock: &dyn coven::Clock) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(clock.now())
}

struct DatabaseInner {
    /// The top-level coven handle owns the connection and exposes the host SQL
    /// path. Writes to synced tables are captured by coven's attached session.
    handle: CovenHandle,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    clock: ClockRef,
    /// Where a row a write makes takes its fresh id: a new label known only
    /// by its name.
    ids: coven::IdSourceRef,
    profile_creation: tokio::sync::Mutex<()>,
    history_registration: tokio::sync::Mutex<()>,
}

/// Database client over coven's owned connection. Writes to synced tables are
/// captured by coven's attached session for changeset sync.
///
/// coven also owns connection pragmas such as `foreign_keys` and `journal_mode`:
/// bae never opens a production SQLite connection or sets a production connection
/// pragma, it inherits those guarantees from the coven handle.
#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    // ── Database method conventions ───────────────────────────────────────
    //
    // Lookup methods come in two shapes, picked by where the ID came from. Using
    // the wrong one is a bug.
    //
    // 1. `find_*` — the ID came from the caller (a screen's action, an API parameter, a
    //    user-provided string). The row may not exist; the user could be looking
    //    at something since deleted. Returns `Result<Option<T>>`.
    //
    // 2. `get_*_for_*` — you're following a foreign key off a record you already
    //    read from the DB. The row MUST exist; if it doesn't, our data integrity
    //    is broken. Returns `Result<T>`, NOT Option, and takes the parent record
    //    rather than a raw ID string so it can't be called with a caller-provided
    //    ID. A missing row surfaces as `QueryReturnedNoRows`.

    fn coven_error(error: CovenError) -> DbError {
        error.into()
    }

    // Reads release their connection before optional result processing. Every
    // write commits before its future resolves; an error rolls back the write.

    /// Run a pure read on Coven's bounded readers. Attach `.process()` for
    /// result assembly after releasing the connection.
    pub fn read<R>(
        &self,
        f: impl for<'conn> FnOnce(SqlReadContext<'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> database_read::DatabaseRead<
        '_,
        impl for<'conn> FnOnce(SqlReadContext<'conn>) -> Result<R, CovenError> + Send + 'static,
    >
    where
        R: Send + 'static,
    {
        database_read::DatabaseRead::new(
            self.inner.handle.read(move |sql| f(sql).map_err(CovenError::from)),
        )
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
        self.write_coven(move |sql| f(sql).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
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
        self.inner
            .handle
            .write_bulk(move |sql| f(&sql).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
    }

    /// A live query: coven reruns `query` for each write that touches what it
    /// read.
    pub fn subscribe<F, R>(&self, query: F) -> coven::LiveQuery<R>
    where
        F: Fn(SqlReadContext<'_>) -> Result<R, CovenError> + Send + Sync + 'static,
        R: Send + 'static,
    {
        self.inner.handle.subscribe(query)
    }

    /// A live query whose request can be replaced without a new
    /// subscription.
    pub fn subscribe_reconfigurable<Q, F, R>(
        &self,
        initial_request: Q,
        query: F,
    ) -> coven::ReconfigurableLiveQuery<Q, R>
    where
        Q: Clone + PartialEq + Send + Sync + 'static,
        F: Fn(&Q, SqlReadContext<'_>) -> Result<R, CovenError> + Send + Sync + 'static,
        R: Send + 'static,
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
    /// async stack, including the import worker's stack.
    fn write_coven<R>(
        &self,
        write: impl for<'ctx> FnOnce(CovenWrite<'ctx>) -> Result<R, CovenError> + Send + 'static,
    ) -> impl std::future::Future<Output = Result<R, CovenError>> + Send + '_
    where
        R: Send + 'static,
    {
        Box::pin(self.inner.handle.write(write))
    }

    /// Stage app bytes, then run a write that puts them in its rows: `stage`
    /// returns what the write needs, such as the ids staging gave the files,
    /// and `sql` gets it once the write has taken the staged files. The
    /// staging future is boxed, as the write's is, to keep coven's buffers off
    /// the caller's async stack.
    pub async fn write_staged<T, R>(
        &self,
        stage: impl FnOnce(&mut coven::WriteBatch<'_>) -> Result<T, CovenError> + Send + 'static,
        sql: impl for<'ctx> FnOnce(CovenWrite<'ctx>, T) -> Result<R, CovenError> + Send + 'static,
    ) -> Result<R, CovenError>
    where
        T: Send + 'static,
        R: Send + 'static,
    {
        let staged = Box::pin(self.inner.handle.stage_files(stage)).await?;
        self.write_coven(move |write| {
            let value = write.put_staged(staged)?;
            sql(write, value)
        })
        .await
    }

    pub fn from_handle(handle: CovenHandle, clock: ClockRef, ids: coven::IdSourceRef) -> Self {
        Database {
            inner: Arc::new(DatabaseInner {
                handle,
                clock,
                ids,
                profile_creation: tokio::sync::Mutex::new(()),
                history_registration: tokio::sync::Mutex::new(()),
            }),
        }
    }

    /// Create an isolated installation with bae's real schema and declarations.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test(database_path: &str, clock: ClockRef) -> Result<Self, DbError> {
        let root = Path::new(database_path)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| {
                DbError::Message(format!("database path has no parent: {database_path}"))
            })?;
        let installation = coven::TestCoven::new();
        let handle = crate::configure_store(
            installation.builder(bae_app_dir::AppDir::at(root)?.store_layout()),
        )
        .clock(clock.clone())
        .create_store("Test Library")
        .await?;
        Ok(Self::from_handle(handle, clock, Arc::new(coven::UuidIds)))
    }
}

fn uuid_column(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value).map_err(|error| read::column_conversion_error(row, index, error))
}
