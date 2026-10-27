use crate::credits::LibraryCredit;
use crate::db::models::*;
// The catalogs a record names belong to the import path, which the mobile
// builds do not carry; the modules that read one are gated with it.
use crate::queue::QueueItem;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use bae_catalogs::catalog::Catalog;
use bae_util::content_type::ContentType;
use chrono::{DateTime, Utc};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use rusqlite::named_params;
use rusqlite::{params, OptionalExtension, Params, Row};
// coven owns production connections; row-mapping tests also use bare SQLite.
use crate::db::DbError;
use coven::{ClockRef, CovenError, CovenHandle, SqlContext, SqlReadContext};
#[cfg(test)]
use rusqlite::Connection;
use std::collections::{BTreeSet, HashMap, HashSet};
// Only the test-only external-ref helper names a path type here; production
// paths live on the types that carry them.
#[cfg(any(test, feature = "test-utils"))]
use std::path::Path;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::str::FromStr;
use std::sync::Arc;
use tracing::warn;

mod album;
pub use album::{AlbumBrowseProjection, AlbumDetailProjection, LibrarySearchProjection};
pub use artist::{
    ArtistBrowseProjection, ArtistDetailProjection, ComposerBrowseProjection,
    ComposerDetailProjection, WorkDetailProjection,
};
mod artist;
mod artist_credit_names;
use artist_credit_names::settle_credit_name;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod artist_identity_choice;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod artist_identity_merge;
mod artist_resolution;
#[cfg(test)]
pub(crate) use artist_resolution::resolved_in_an_empty_library;
pub(crate) use artist_resolution::ArtistCredits;
pub use artist_resolution::ArtistWriteError;
use artist_resolution::{relink_album_artists, relink_track_artists};
// Only the import write resolves credits inside a release's own write.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use artist_resolution::{artist_write_failure, relink};
mod abandoned;
pub(crate) use abandoned::AbandonedRow;
mod file_row;
pub(crate) use file_row::FileRow;
mod pending;
pub(crate) use pending::PendingRow;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod candidate_state_rows;
mod coven_capabilities;
mod database_read;
mod files;
mod records;
// Only identifying a release, a desktop operation, changes its records.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use records::RecordsChange;
// Watched folders, folder scans and the import candidate queue. Read
// `import::watched_folder` and `import::FolderScanStatus`, both desktop-only,
// and every caller is a gated import module — the mobile builds are sync and
// playback clients with no import pipeline.
#[cfg(desktop)]
mod folder_scans;
#[cfg(desktop)]
pub use folder_scans::{FinishedScan, ScanItemWrite};
#[cfg(desktop)]
pub(crate) use folder_scans::{
    FolderReadingCommit, FolderReadingStamp, FolderReadingWrite, ScanItemSeed, ScanItemToWrite,
};
#[cfg(desktop)]
mod release_groupings;
#[cfg(desktop)]
pub use release_groupings::GroupingChanges;
#[cfg(desktop)]
pub(crate) use release_groupings::GroupingFacts;
#[cfg(desktop)]
mod import_content_hash;
#[cfg(desktop)]
mod import_list;
#[cfg(desktop)]
mod import_state;
#[cfg(desktop)]
pub(crate) use import_list::SelectionWrite;
#[cfg(desktop)]
pub(crate) use import_state::{
    CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved, ImportClaim,
    RunVerdictSaved, ScannedCandidateKey, StandingsRead,
};
#[cfg(desktop)]
mod album_link_rows;
mod collections;
mod history;
mod people;
mod playback;
mod playback_queue;
pub use playback_queue::{
    ContextFill, ContextFilled, ManualPlace, PlaybackQueueTables, ReleaseStart, ShuffleChange,
};
mod playback_queue_reads;
#[cfg(desktop)]
mod source_releases;
pub(crate) use files::{OutboxDisplayContext, OutboxDisplayRequest};
pub(crate) use playback_queue_reads::QueueUpcomingRead;
mod release;
pub(crate) use release::ReleaseMetadataRows;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use release::{ImportRows, NewArtistImages};
// Only saving a release edit session writes metadata alongside records.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use release::write_release_metadata_on;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod release_edit_save;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use release::AlbumFields;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use release_edit_save::ReleaseEditWrite;
mod release_file;
mod release_locality;
pub(crate) use release_file::ReleaseFileStream;
mod release_deletion;
pub use release_deletion::ReleaseDeletion;
mod release_projection;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
mod seeded_albums;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(crate) use seeded_albums::{SeededAlbum, SeededAudio};
#[cfg(any(test, feature = "test-utils"))]
mod test_capabilities;
mod track;

#[cfg(test)]
mod tests;

mod pressing_columns;
mod query;
mod read;
mod scrub_points_column;
mod write;

use query::*;
use read::*;
use release_projection::{
    find_release_detail_context_on, storage_count_on, storage_page_on, storage_selected_on,
    storage_total_size_on,
};
pub use release_projection::{
    ReleaseDetailProjection, StorageBrowseProjection, StorageBrowseRequest,
};
use write::*;

struct DatabaseInner {
    /// The top-level coven handle owns the connection and exposes the host SQL
    /// path. Writes to synced tables are captured by coven's attached session.
    handle: CovenHandle,
    /// Wall clock for `created_at` and status timestamps bound into write SQL.
    clock: ClockRef,
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
        error.into()
    }

    // Reads release their connection before optional result processing. Every
    // write commits before its future resolves; an error rolls back the write.

    /// Run a pure read on Coven's bounded readers. Attach `.process()` for
    /// result assembly after releasing the connection.
    fn read<R>(
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
            self.inner
                .handle
                .read(move |sql| f(sql).map_err(CovenError::from)),
        )
    }

    /// Run a write with a borrowed transaction context.
    async fn call<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(&SqlContext<'ctx, 'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.call_sql(move |sql| f(&sql)).await
    }

    /// Run an atomic write through coven.
    async fn call_sql<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(SqlContext<'ctx, 'conn>) -> Result<R, DbError> + Send + 'static,
    ) -> Result<R, DbError>
    where
        R: Send + 'static,
    {
        self.write_with_files(|_| Ok(()), move |sql| f(sql).map_err(CovenError::from))
            .await
            .map_err(Self::coven_error)
    }

    /// Keep coven's file-staging buffers off the caller's async stack, including
    /// the import worker's stack. Row-only writes use this same boundary.
    fn write_with_files<R>(
        &self,
        build: impl FnOnce(&mut coven::WriteBatch) -> Result<(), CovenError> + Send + 'static,
        sql: impl for<'ctx, 'conn> FnOnce(SqlContext<'ctx, 'conn>) -> Result<R, CovenError>
            + Send
            + 'static,
    ) -> impl std::future::Future<Output = Result<R, CovenError>> + Send + '_
    where
        R: Send + 'static,
    {
        Box::pin(self.inner.handle.write_with_files(build, sql))
    }

    pub fn from_handle(handle: CovenHandle, clock: ClockRef) -> Self {
        Database {
            inner: Arc::new(DatabaseInner {
                handle,
                clock,
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
        let handle = crate::library::configure_store(
            installation.builder(crate::config::AppDir::at(root)?.store_layout()),
        )
        .clock(clock.clone())
        .create_store("Test Library")
        .await?;
        Ok(Self::from_handle(handle, clock))
    }
}

fn uuid_column(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
