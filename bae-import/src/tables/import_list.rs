//! The import tab's list, read from its placement.
//!
//! Each list a surface has open is a row of `import_list`, beside the groups
//! it folds (`import_list_collapsed`) and the rows it selects
//! (`import_list_selection`); every release's tab, Found state, group, dates,
//! shown text and command basis are in `import_list_placement`, which triggers
//! keep in the write that changes any table they come from. A surface follows
//! the list by live queries of their own (`read`): each window, a range of the
//! list's order (`order`) whose rows are read by key (`window`), the count and
//! the chrome (`summary`), and what the selection holds and offers
//! (`selection`). A search reads the rows its index (`import_list_search`)
//! finds. Nothing is placed, filtered, sorted or counted outside SQLite. The pane reads one
//! candidate (`pane`) from the same placement. A surface changes the list it
//! has open by its view and folds (`writes`) and by its selection
//! (`selection`).
//!
//! Its schema is the SQL files beside
//! this one, a file per family of rules: the placement and the rules it is
//! kept by (`placement`), Found's groups (`groups`), the counts (`counts`),
//! the search index (`search`), the lists a surface has open (`views`), the
//! triggers that place a release again when a table it is kept from changes
//! (`follows_scans`, `follows_groupings`, `follows_candidates`,
//! `follows_library`).

mod counts;
mod follows_candidates;
mod follows_groupings;
mod follows_library;
mod follows_scans;
mod groups;
mod listed;
mod order;
mod pane;
mod placement;
mod read;
mod search;
mod selection;
mod summary;
mod views;
mod window;
mod writes;

use super::*;
use crate::list::{ImportListId, ImportListView};
use crate::PendingFilter;
use order::{Items, Shown};

pub use selection::SelectionWrite;

/// Why a read or a write of one open list failed.
#[derive(Debug, thiserror::Error)]
pub enum ImportListError {
    /// The list is no longer open: the surface that opened it closed it.
    #[error("import list {0:?} is not open")]
    Closed(ImportListId),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A store failure the list's live query met is the store's failure.
impl From<coven::DbError> for ImportListError {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

/// A bug a run of the list's live query carries back: the store's.
impl coven::task::CarriesBug for ImportListError {
    fn bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Closed(_) => None,
            Self::Database(error) => error.bug(),
        }
    }
}

/// The import list's part of the library's schema. Each file runs after the
/// ones creating the tables its triggers fire on: the placement first, the
/// lists a surface has open before the triggers that follow other domains'
/// tables.
pub(super) const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[
    placement::SQL,
    groups::SQL,
    counts::SQL,
    search::SQL,
    views::SQL,
    follows_scans::SQL,
    follows_groupings::SQL,
    follows_candidates::SQL,
    follows_library::SQL,
])
.functions(SQL_FUNCTIONS)
.collations(SQL_COLLATIONS);

/// The SQL functions the import list's SQL names.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[
    &bae_store::FOLD_CASE,
    &bae_store::SEARCH_PATTERN,
    &super::watched_path::FOLDER_BELOW,
    &super::watched_path::FOLDER_RELATIVE,
];

/// The collations the import list's SQL names.
const SQL_COLLATIONS: &[&bae_store::SchemaCollation] = &[&bae_store::NATURAL_ORDER];

/// One list's row as its reads take it: its view, and whether it narrows its
/// tab or keeps every row of it.
#[derive(Debug, Clone, PartialEq)]
struct ListRow {
    id: ImportListId,
    view: ImportListView,
    /// Whether the text filter or Found's filter entry narrows the tab.
    narrows: bool,
    /// Whether the text filter narrows it, which only a read of every row's
    /// text can count.
    filters_text: bool,
    /// Whether it shows every row of its tab: nothing narrows it and no
    /// group is folded shut.
    keeps_all: bool,
}

impl ListRow {
    fn items<'a>(&'a self, shown: Shown<'a>) -> Items<'a> {
        Items::new(self.id, self.view.tab, self.view.order, shown)
    }
}

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

/// The list `id`'s row; `None` once the list is closed. A write reads it
/// before anything else, so a fold or a selection never names a list that is
/// gone.
fn list_row(sql: &impl coven::SqlRead, id: ImportListId) -> Result<Option<ListRow>, DbError> {
    let Some(((tab, pending_filter, filter_text, order), (narrows, filters_text, keeps_all))) = sql
        .query_row(
            "SELECT tab, pending_filter, filter_text, list_order, narrows, \
                    pattern IS NOT NULL, keeps_all \
             FROM import_list_view WHERE id = ?",
            [id.0],
            |row| {
                Ok((
                    (
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ),
                    (row.get::<_, bool>(4)?, row.get::<_, bool>(5)?, row.get::<_, bool>(6)?),
                ))
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    Ok(Some(ListRow {
        id,
        view: ImportListView {
            tab: views::tab_of(&tab)?,
            filter_text,
            pending_filter: pending_filter_of(&pending_filter)?,
            order: views::order_of(&order)?,
        },
        narrows,
        filters_text,
        keeps_all,
    }))
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
pub(crate) mod tests;
