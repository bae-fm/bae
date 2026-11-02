//! Import's own tables in the library's store: the folder scans, the import
//! list, each candidate's stored state, and the releases groupings make.
//! Their SQL is functions over coven's write and read contexts, so an import
//! commit runs them and db's row writers in one transaction; [`ImportTables`]
//! runs them through the store for everything else.

mod album_link_rows;
mod candidate_state_rows;
mod downscaled_copies;
mod folder_scans;
mod import_content_hash;
mod import_list;
pub(crate) mod import_state;
mod release_groupings;
mod source_releases;

pub use folder_scans::{FinishedScan, ScanItemWrite};
pub(crate) use folder_scans::{
    FolderReadingCommit, FolderReadingWrite, ScanItemSeed, ScanItemToWrite,
};
pub(crate) use import_list::SelectionWrite;
pub use import_state::ImportCommitError;
pub(crate) use import_state::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, ImportClaim, ImportCommitGuard, ImportRows, NewArtistImages, RunVerdictSaved,
    ScannedCandidateKey,
};
pub use release_groupings::GroupingChanges;
pub(crate) use release_groupings::GroupingFacts;

use crate::db::client::list_windows::{
    list_order_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Places, Seek, SortTerm,
    WindowsRead,
};
use crate::db::client::path_column;
use crate::db::client::query::*;
use crate::db::client::read::*;
use crate::db::*;
use bae_catalogs::catalog::Catalog;
use coven::{CovenError, CovenWrite, SqlReadContext};
use rusqlite::{named_params, params, OptionalExtension, Row};
use std::collections::HashMap;
use std::str::FromStr;

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
