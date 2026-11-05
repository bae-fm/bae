//! Import's own tables in the library's store: the folder scans, the import
//! list, each candidate's stored state, and the releases groupings make.
//! Their SQL is functions over coven's write and read contexts, so an import
//! commit runs them and db's row writers in one transaction; [`ImportTables`]
//! runs them through the store for everything else.

mod album_link_rows;
mod downscaled_copies;
mod folder_scans;
mod import_list;
mod import_origin;
pub(crate) mod import_state;
mod release_groupings;
mod skipped_candidates;
mod source_releases;
mod watched_folders;

/// Import's parts of the library's schema, in the order they run.
pub const SCHEMA_PARTS: &[bae_db::SchemaPart] = &[
    watched_folders::SCHEMA,
    skipped_candidates::SCHEMA,
    folder_scans::SCHEMA,
    release_groupings::SCHEMA,
    import_state::SCHEMA,
    source_releases::SCHEMA,
    import_origin::SCHEMA,
    import_list::SCHEMA,
];

pub(crate) use folder_scans::DbCandidateFileTagSnapshot;
pub use folder_scans::{FinishedScan, ScanItemWrite};
pub(crate) use folder_scans::{
    FolderEntryToWrite, FolderReadingCommit, FolderReadingWrite, ScanItemSeed, ScanItemToWrite,
};
pub use import_list::SelectionWrite;
pub(crate) use import_state::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CommittingCandidate, ImportClaim, ImportCommitGuard, ImportRows,
    NewArtistImages, RunVerdictSaved, ScannedCandidateKey,
};
pub use import_state::{
    DbCandidateIdentifyResult, DbCandidatePaneRows, DbImportCandidateState, ImportCommitError,
    NewImportCandidateVerdict, VerdictPick,
};
pub use release_groupings::GroupingChanges;
pub(crate) use release_groupings::GroupingFacts;
pub use watched_folders::watched_path;
pub use watched_folders::WatchedFoldersRemoved;

use bae_catalogs::catalog::Catalog;
use bae_db::client::list_windows::{
    list_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Places, Seek, SortTerm,
    WindowsRead,
};
use bae_db::client::path_column;
use bae_db::client::query::*;
use bae_db::client::read::*;
use bae_db::*;
use coven::{CovenError, CovenWrite, SqlReadContext};
use rusqlite::{named_params, params, OptionalExtension, Row};
use std::collections::HashMap;
use std::str::FromStr;

/// What a read fetched, still to be assembled — boxed so a read on either
/// connection hands back one type.
type FinishRead<T> = Box<dyn FnOnce() -> Result<T, DbError> + Send + 'static>;

/// `keys` as a JSON array of their bytes in hex, for a query that matches a
/// key column with `unhex(value)` over `json_each`.
fn key_hex_list<'a>(keys: impl Iterator<Item = &'a crate::CandidateKey>) -> String {
    serde_json::to_string(&keys.map(|key| hex::encode(key.as_bytes())).collect::<Vec<_>>())
        .expect("hex text serializes as JSON")
}

/// The library's store, as import reads and writes its own tables in it.
#[derive(Clone, Debug)]
pub struct ImportTables {
    database: Database,
}

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
