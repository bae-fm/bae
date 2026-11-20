//! Import's own tables in the library's store: the folder scans, the import
//! list, each candidate's stored state, and the releases groupings make.
//! Their SQL is functions over coven's write and read contexts, so an import
//! commit runs them and db's row writers in one transaction; [`ImportTables`]
//! runs them through the store for everything else.

mod album_link_rows;
mod catalog_apply_steps;
mod cover_gallery;
mod cover_labels;
mod credit_resolutions;
mod downscaled_copies;
mod draft_rows;
mod folder_scans;
mod import_list;
mod import_origin;
pub(crate) mod import_state;
mod reading_refusals;
mod release_edit;
mod release_groupings;
mod release_sheet_decisions;
mod sheet_decision_columns;
mod skipped_candidates;
mod source_releases;
mod stored_credits;
mod trim;
mod watched_folders;

/// Import's parts of the library's schema, in the order they run.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[
    watched_folders::SCHEMA,
    skipped_candidates::SCHEMA,
    folder_scans::SCHEMA,
    release_groupings::SCHEMA,
    import_origin::SCHEMA,
    release_sheet_decisions::SCHEMA,
    import_state::SCHEMA,
    source_releases::SCHEMA,
    import_list::SCHEMA,
    cover_gallery::SCHEMA,
    catalog_apply_steps::SCHEMA,
    credit_resolutions::SCHEMA,
];

pub use cover_gallery::{
    CoverFetch, CoverFilter, CoverGallery, CoverGalleryOwner, CoverGalleryRequest,
    CoverGallerySnapshot, CoverKey, CoverSection, GalleryCover, GalleryFocus, GalleryLabel,
    Neighbors,
};
pub(crate) use folder_scans::DbCandidateFileTagSnapshot;
pub use folder_scans::{FinishedScan, FolderRereadRefusal, ScanEntryMoved, ScanItemWrite};
pub(crate) use folder_scans::{
    FolderEntryToWrite, FolderReadingCommit, FolderReadingWrite, ScanItemReadings, ScanItemSeed,
    ScanItemToWrite, ScanWriteError,
};
pub use import_list::{FolderImageFocus, FolderImages, FolderImagesRequest, FolderImagesSnapshot};
pub(crate) use import_state::{
    CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved,
    CommittingCandidate, ImportClaim, ImportCommitGuard, ImportRows, NewAlbum, NewArtistImages,
    RunVerdictSaved, ScannedCandidateKey,
};
pub use import_state::{
    CandidateStateRefusal, DbCandidateIdentifyResult, DbCandidatePaneRows, DbImportCandidateState,
    ImportCommitError, ImportCommitRefusal, NewImportCandidateVerdict, PreparationRefusal,
    VerdictPick,
};
pub use release_edit::{
    release_edit_form_on, release_edit_layout_on, release_edit_still_as_opened_on,
    store_release_edit_sheet_decisions_on, ReleaseEditContext, ReleaseEditSeedRefusal,
};
pub(crate) use release_groupings::GroupingFacts;
pub use release_groupings::{FolderReadingRefusal, GroupingChanges, SeparationRefusal};
pub use watched_folders::watched_path;
pub use watched_folders::{WatchedFolderRefusal, WatchedFoldersRemoved};

use bae_catalogs::catalog::Catalog;
use bae_records::*;
use bae_store::list_windows::{ListOrder, Seek, SortTerm};
use bae_store::path_column;
use bae_store::query::*;
use bae_store::read::*;
use bae_store::*;
use coven::{CovenWrite, SqlReadContext};
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
    store: Store,
}

impl ImportTables {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// How many rows of `table` have `column` equal to `value`, for a test.
    #[cfg(test)]
    pub(crate) async fn count_rows_for_test(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<i64, DbError> {
        self.store.count_rows_for_test(table, column, value).await
    }
}

#[cfg(test)]
#[path = "tables_tests.rs"]
pub(crate) mod tests;
