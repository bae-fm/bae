//! Import's own tables in the library's store: the folder scans, the import
//! list, each candidate's stored state, and the releases groupings make.
//! Their SQL is functions over coven's write and read contexts, so an import
//! commit runs them and db's row writers in one transaction; [`ImportTables`]
//! runs them through the store for everything else. Their schema is every
//! device's, since every device opens the same library; the rest is desktop's.

#[cfg(desktop)]
mod album_link_rows;
#[cfg(desktop)]
mod downscaled_copies;
#[cfg(desktop)]
mod folder_scans;
mod import_list;
#[cfg(desktop)]
mod import_origin;
#[cfg(desktop)]
pub(crate) mod import_state;
#[cfg(desktop)]
mod release_groupings;
mod skipped_candidates;
#[cfg(desktop)]
mod source_releases;
mod watched_folders;

/// Import's parts of the library's schema, in the order they run.
pub const SCHEMA_PARTS: &[bae_db::SchemaPart] =
    &[watched_folders::SCHEMA, skipped_candidates::SCHEMA, import_list::SCHEMA];

#[cfg(desktop)]
pub(crate) use folder_scans::DbCandidateFileTagSnapshot;
#[cfg(desktop)]
pub use folder_scans::{FinishedScan, ScanItemWrite};
#[cfg(desktop)]
pub(crate) use folder_scans::{
    FolderEntryToWrite, FolderReadingCommit, FolderReadingWrite, ScanItemSeed, ScanItemToWrite,
};
#[cfg(desktop)]
pub use import_list::SelectionWrite;
#[cfg(desktop)]
pub(crate) use import_state::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CommittingCandidate, ImportClaim, ImportCommitGuard, ImportRows,
    NewArtistImages, RunVerdictSaved, ScannedCandidateKey,
};
#[cfg(desktop)]
pub use import_state::{
    DbCandidateIdentifyResult, DbCandidatePaneRows, DbImportCandidateState, ImportCommitError,
    NewImportCandidateVerdict, VerdictPick,
};
#[cfg(desktop)]
pub use release_groupings::GroupingChanges;
#[cfg(desktop)]
pub(crate) use release_groupings::GroupingFacts;
#[cfg(desktop)]
pub use watched_folders::WatchedFoldersRemoved;

#[cfg(desktop)]
use bae_catalogs::catalog::Catalog;
#[cfg(desktop)]
use bae_db::client::list_windows::{
    list_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Places, Seek, SortTerm,
    WindowsRead,
};
#[cfg(desktop)]
use bae_db::client::path_column;
#[cfg(desktop)]
use bae_db::client::query::*;
#[cfg(desktop)]
use bae_db::client::read::*;
#[cfg(desktop)]
use bae_db::*;
#[cfg(desktop)]
use coven::{CovenError, CovenWrite, SqlReadContext};
#[cfg(desktop)]
use rusqlite::{named_params, params, OptionalExtension, Row};
#[cfg(desktop)]
use std::collections::HashMap;
#[cfg(desktop)]
use std::str::FromStr;

/// What a read fetched, still to be assembled — boxed so a read on either
/// connection hands back one type.
#[cfg(desktop)]
type FinishRead<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

/// `keys` as a JSON array of their bytes in hex, for a query that matches a
/// key column with `unhex(value)` over `json_each`.
#[cfg(desktop)]
fn key_hex_list<'a>(keys: impl Iterator<Item = &'a crate::CandidateKey>) -> String {
    serde_json::to_string(&keys.map(|key| hex::encode(key.as_bytes())).collect::<Vec<_>>())
        .expect("hex text serializes as JSON")
}

/// The library's store, as import reads and writes its own tables in it.
#[cfg(desktop)]
#[derive(Clone, Debug)]
pub struct ImportTables {
    database: Database,
}

#[cfg(desktop)]
impl ImportTables {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Run `statements` against the store, for a test that stores the rows a
    /// scan would.
    #[cfg(test)]
    pub(crate) async fn execute_local_sql_for_test(&self, statements: &str) -> Result<(), DbError> {
        self.database.execute_local_sql_for_test(statements).await
    }

    /// How many rows of `table` have `column` equal to `value`, for a test.
    #[cfg(test)]
    pub(crate) async fn count_rows_for_test(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<i64, DbError> {
        self.database.count_rows_for_test(table, column, value).await
    }
}

#[cfg(test)]
pub(crate) mod tests;
