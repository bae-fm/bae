use crate::db::models::*;
use crate::import::MetadataSource;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use coven::rusqlite::named_params;
use coven::rusqlite::{params, OptionalExtension, Params, Row};
// bae holds no production connection — coven owns them all. Only the cloud-path
// resolvers' unit tests seed a bare one to run against.
#[cfg(test)]
use coven::rusqlite::Connection;
#[cfg(any(test, feature = "test-utils"))]
use coven::Coven;
use coven::{ClockRef, CovenError, CovenHandle, DbError, IdRef, SqlContext, SqlReadContext};
use std::collections::{BTreeSet, HashMap};
// Used only by the desktop-only import modules below, which import `super::*`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::collections::HashSet;
// Only the test-only external-ref helper names a path type here; production
// paths live on the types that carry them.
#[cfg(any(test, feature = "test-utils"))]
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::sync::Mutex;
use tracing::warn;

mod album;
pub use album::{
    AlbumBrowseProjection, AlbumDetailProjection, AlbumPageProjection, LibrarySearchProjection,
};
pub use artist::{
    ArtistDetailProjection, ArtistPageProjection, ComposerBrowseProjection,
    ComposerDetailProjection, ComposerPageProjection, WorkDetailProjection,
};
mod artist;
mod blobs;
mod coven_capabilities;
mod identity;
// Watched folders, folder scans and the import candidate queue. Reads
// `import::folder_registry` and `import::FolderScanStatus`, both desktop-only,
// and every caller is a gated import module — the mobile builds are sync and
// playback clients with no import pipeline.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod folder_scans;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import_content_hash;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import_list;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import_state;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use import_list::{
    CandidateStateListRow, ImportQueueRows, ScanBoundaryListRow, ScanCandidateKind,
    ScanCandidateListRow,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod payloads;
mod playback;
pub(crate) use playback::QueueCatalogProjection;
mod release;
mod release_projection;
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;
mod track;

#[cfg(test)]
mod tests;

mod query;
mod read;
mod write;

use query::*;
pub use query::{DeleteCleanupPlan, ImportReplacementDelete, ImportReplacementOutcome};
use read::*;
use release_projection::{
    find_release_detail_context_on, storage_count_on, storage_page_on, storage_total_size_on,
};
pub use release_projection::{ReleaseDetailProjection, StoragePageProjection};
use write::*;

struct DatabaseInner {
    /// The top-level coven handle owns the connection and exposes the host SQL
    /// path. Writes to synced tables are captured by coven's attached session.
    handle: CovenHandle,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    /// Synced-table `_updated_at` is stamped from coven's SQL context.
    clock: ClockRef,
    /// The id source for the few rows this layer mints itself — the ones whose
    /// count is only known inside the transaction that writes them (a release's
    /// identity rows, an album's copied `album_artists`). Every other id is minted
    /// by the caller from the same provider and passed in, so the DB holding one
    /// keeps every id in the process coming from the one injected source.
    ids: IdRef,
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
    // 1. `find_*` — the ID came from the caller (a UI event, an API parameter, a
    //    user-provided string). The row may not exist; the user could be looking
    //    at something since deleted. Returns `Result<Option<T>>`.
    //
    // 2. `get_*_for_*` — you're following a foreign key off a record you already
    //    read from the DB. The row MUST exist; if it doesn't, our data integrity
    //    is broken. Returns `Result<T>`, NOT Option, and takes the parent record
    //    rather than a raw ID string so it can't be called with a caller-provided
    //    ID. A missing row surfaces as `QueryReturnedNoRows`.

    fn coven_error(error: CovenError) -> DbError {
        match error {
            CovenError::Database(error) => *error,
            other => DbError::Message(other.to_string()),
        }
    }

    // ── The three SQL entry points ────────────────────────────────────────
    //
    // `read`     — pure reads, on coven's read-only companion connection: no
    //              changeset journal, concurrent with the writer rather than
    //              queued behind it. The closure cannot write — a
    //              `SqlReadContext` offers only `query`/`query_row`, and the
    //              connection behind it is SQLITE_OPEN_READONLY.
    // `call`     — writes that do not stamp `_updated_at` (INSERT/DELETE, or an
    //              UPDATE that sets the register clock itself), on the writer
    //              connection with a changeset session attached.
    // `call_sql` — writes that stamp `_updated_at` from coven's SQL context.
    //
    // Read-your-writes holds: every `call`/`call_sql` write commits before its
    // future resolves and the WAL reader sees the last committed state, so a
    // `read` after an awaited write is correct. A closure that reads and then
    // conditionally writes is a write — it belongs on `call`, not `read`.

    /// Run a pure read on coven's read-only companion connection. See the entry
    /// points note above.
    async fn read<R>(
        &self,
        f: impl for<'conn> FnOnce(SqlReadContext<'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .read(move |sql| f(sql).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
    }

    /// Run a write that does not stamp `_updated_at`. See the entry points note
    /// above.
    async fn call<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(&SqlContext<'ctx, 'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.call_sql(move |sql| f(&sql)).await
    }

    /// Run a write that stamps `_updated_at` from coven's SQL context. See the
    /// entry points note above.
    async fn call_sql<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(SqlContext<'ctx, 'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .write(move |sql| f(sql).map_err(CovenError::from))
            .await
            .map(|receipt| receipt.value)
            .map_err(Self::coven_error)
    }

    pub fn from_handle(handle: CovenHandle, clock: ClockRef, ids: IdRef) -> Self {
        Database {
            inner: Arc::new(DatabaseInner { handle, clock, ids }),
        }
    }

    /// Open the database through coven's top-level builder, running coven's
    /// bookkeeping migration plus bae's schema.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn open(
        store_dir: coven::StoreDir,
        config: impl Into<coven::CovenConfig>,
        clock: ClockRef,
        ids: IdRef,
        synced_tables: Vec<coven::SyncedTable>,
        observer: Option<Arc<dyn coven::BlobTransitionObserver>>,
    ) -> Result<Self, DbError> {
        let mut builder = Coven::builder(store_dir, config)
            .synced_tables(synced_tables)
            .clock(clock.clone())
            .oauth_clients(crate::oauth::clients());
        if let Some(observer) = observer {
            builder = builder.observer(observer);
        }
        let handle = builder
            .migrations(crate::migrations::all())
            .open()
            .map_err(Self::coven_error)?;
        Ok(Self::from_handle(handle, clock, ids))
    }

    /// Test convenience: open over `path` with a fresh device id and bae's real
    /// synced-table set, so unit/integration tests don't repeat the wiring.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_test(
        database_path: &str,
        clock: ClockRef,
        ids: IdRef,
    ) -> Result<Self, DbError> {
        tracing::info!("Opening database at {}", database_path);
        let path = Path::new(database_path);
        let library_root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| {
                DbError::Message(format!("database path has no parent: {database_path}"))
            })?;
        let library_dir = coven::StoreDir::new(library_root);
        let config = coven::Config::with_defaults(
            "test-library".to_string(),
            "test-device".to_string(),
            "Test Library".to_string(),
        );
        // Coven's custody owners capture the registered keyring service when
        // the builder opens, so install the test service first.
        crate::config::install_test_keyring();
        Self::open(
            library_dir,
            config,
            clock,
            ids,
            crate::sync::synced_tables(),
            None,
        )
    }
}
