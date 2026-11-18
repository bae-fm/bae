//! The import tab's list, read from its placement.
//!
//! Every release's tab, Found state, group, dates, shown text and command
//! basis are in `import_list_placement`, which triggers keep in the write that
//! changes any table they come from. The surface keeps the list's view and
//! the groups it folds, and each read takes them as its request
//! (`ImportListRequest`), so no table stores them. A surface runs a live
//! query (`read`) for each window (a range of the list's order, `order`,
//! whose rows `window` reads by key), for the count and summary (`summary`),
//! and for what the selected rows hold and offer (`selection`); the surface
//! keeps the selection itself. A search reads the rows its index
//! (`import_list_search`) finds. SQLite places, filters, sorts and counts every
//! row; no Rust code does. The pane reads one candidate (`pane`) from the same
//! placement.
//!
//! Its schema is the SQL files beside this one, a file per family of rules:
//! the placement and the rules it is kept by (`placement`), Found's groups
//! (`groups`), the counts (`counts`), the search index (`search`), the
//! triggers that place a release again when a table it is kept from changes
//! (`follows_scans`, `follows_groupings`, `follows_candidates`,
//! `follows_library`).

mod actions;
mod artist_standings;
pub(crate) use artist_standings::{
    album_artist_standing_on, artist_field_standing_of_json, artist_standing_clauses,
    track_artist_standing_sql, CreditSource, FieldStandingJson,
};
mod folder_images;
mod follows_candidates;
mod follows_groupings;
mod follows_library;
mod follows_scans;
mod groups;
mod judged;
mod listed;
mod mapping_rows;
mod order;
mod pane;
mod pane_files;
mod placement;
mod read;
mod search;
mod selection;
mod summary;
mod window;

use super::*;
use crate::list::{ImportListFolds, ImportListRequest};
use crate::{PendingFilter, TriageTab};
use order::Items;

pub use folder_images::{
    FolderImageFocus, FolderImages, FolderImagesRequest, FolderImagesSnapshot,
};

/// The import list's part of the library's schema. Each file runs after the
/// ones creating the tables its triggers fire on: the placement first, then
/// the triggers that follow other domains' tables.
pub(super) const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[
    placement::SQL,
    groups::SQL,
    search::SQL,
    follows_scans::SQL,
    follows_groupings::SQL,
    follows_candidates::SQL,
    follows_library::SQL,
])
.functions(SQL_FUNCTIONS)
.collations(SQL_COLLATIONS);

/// The SQL functions the import's statements name.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[
    &bae_store::FOLD_CASE,
    &bae_store::SEARCH_PATTERN,
    &super::watched_path::FOLDER_BELOW,
    &super::watched_path::FOLDER_RELATIVE,
    &super::layout_text::SIDE_LETTERS,
    &super::layout_text::MEDIUM_IS_SIDED,
    &super::layout_text::LENGTHS_DISAGREE,
    &super::layout_text::TRIM,
    &pane_files::SUPPORTED_COVER,
];

/// The collations the import list's SQL names.
const SQL_COLLATIONS: &[&bae_store::SchemaCollation] = &[&bae_store::NATURAL_ORDER];

fn pending_filter_column(filter: PendingFilter) -> &'static str {
    match filter {
        PendingFilter::All => "all",
        PendingFilter::NeedsYou => "needs_you",
        PendingFilter::InProgress => "in_progress",
        PendingFilter::Identified => "identified",
        PendingFilter::Unmatched => "unmatched",
        PendingFilter::NotLookedUp => "not_looked_up",
    }
}

fn pending_filter_of(column: &str) -> Result<PendingFilter, DbError> {
    match column {
        "all" => Ok(PendingFilter::All),
        "needs_you" => Ok(PendingFilter::NeedsYou),
        "in_progress" => Ok(PendingFilter::InProgress),
        "identified" => Ok(PendingFilter::Identified),
        "unmatched" => Ok(PendingFilter::Unmatched),
        "not_looked_up" => Ok(PendingFilter::NotLookedUp),
        other => Err(DbError::unreadable("an import list's stored pending filter", other)),
    }
}

/// The tab as the placement's `tab` column holds it.
fn tab_column(tab: TriageTab) -> &'static str {
    match tab {
        TriageTab::Pending => "pending",
        TriageTab::Done => "done",
        TriageTab::Skipped => "skipped",
    }
}

fn tab_of(column: &str) -> Result<TriageTab, DbError> {
    match column {
        "pending" => Ok(TriageTab::Pending),
        "done" => Ok(TriageTab::Done),
        "skipped" => Ok(TriageTab::Skipped),
        other => Err(DbError::unreadable("a placement's stored tab", other)),
    }
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
pub(crate) mod tests;
