//! The import tab's list, read from its placement.
//!
//! Each list a surface has open is a row of `import_list`, beside the groups
//! it folds (`import_list_collapsed`) and the rows it selects
//! (`import_list_selection`); every release's tab, Found state, group, dates,
//! shown text and command basis are in `import_list_placement`, which triggers
//! keep in the write that changes any table they come from. A read of the list
//! (`read`) is then a handful of indexed statements: the counts (`summary`),
//! what the selection holds and offers (`selection`), the requested windows in
//! the list's order (`order`), and each window's rows by key (`window`). A
//! search reads the rows its index (`import_list_search`) finds. Nothing is
//! placed, filtered, sorted or counted outside SQLite. The pane reads one
//! candidate (`pane`) from the same placement. A surface changes the list it
//! has open by its view and folds (`writes`) and by its selection
//! (`selection`).
//!
//! Its schema, which every device's library holds, is the SQL files beside
//! this one, a file per family of rules: the placement and the rules it is
//! kept by (`placement`), Found's groups (`groups`), the counts (`counts`),
//! the search index (`search`), the lists a surface has open (`views`), the
//! triggers that place a release again when a table it is kept from changes
//! (`follows_scans`, `follows_groupings`, `follows_candidates`,
//! `follows_library`), and the versions of the tables the list's order reads
//! (`versions`).

mod counts;
mod follows_candidates;
mod follows_groupings;
mod follows_library;
mod follows_scans;
mod groups;
#[cfg(desktop)]
mod listed;
#[cfg(desktop)]
mod order;
#[cfg(desktop)]
mod pane;
mod placement;
#[cfg(desktop)]
mod read;
mod search;
#[cfg(desktop)]
mod selection;
#[cfg(desktop)]
mod summary;
mod versions;
mod views;
#[cfg(desktop)]
mod window;
#[cfg(desktop)]
mod writes;

#[cfg(desktop)]
use super::*;
#[cfg(desktop)]
use crate::list::{ImportListId, ImportListView};
#[cfg(desktop)]
use crate::PendingFilter;
#[cfg(desktop)]
use order::{Items, Shown};

#[cfg(desktop)]
pub use selection::SelectionWrite;

/// The import list's part of the library's schema. Each file runs after the
/// ones creating the tables its triggers fire on: the placement first, the
/// lists a surface has open before the triggers that follow other domains'
/// tables, and the versions after the placement.
pub(super) const SCHEMA: bae_db::SchemaPart = bae_db::SchemaPart::new(&[
    placement::SQL,
    groups::SQL,
    counts::SQL,
    search::SQL,
    views::SQL,
    follows_scans::SQL,
    follows_groupings::SQL,
    follows_candidates::SQL,
    follows_library::SQL,
    versions::SQL,
]);

/// One list's row as its reads take it: its view, its revisions, and whether
/// its view narrows its tab or keeps every row of it.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
struct ListRow {
    id: ImportListId,
    view: ImportListView,
    view_revision: u64,
    selection_revision: u64,
    /// Where its view opened.
    opened_at: u64,
    /// Whether the text filter or Found's filter entry narrows the tab.
    narrows: bool,
    /// Whether the text filter narrows it, which only a read of every row's
    /// text can count.
    filters_text: bool,
    /// Whether it shows every row of its tab: nothing narrows it and no
    /// group is folded shut.
    keeps_all: bool,
}

#[cfg(desktop)]
impl ListRow {
    fn items<'a>(&'a self, shown: Shown<'a>) -> Items<'a> {
        Items::new(self.id, self.view.tab, self.view.order, shown)
    }
}

#[cfg(desktop)]
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

#[cfg(desktop)]
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

/// The list `id`'s row. A list is read only while it is open, so a missing
/// row is a list closed under its own read.
#[cfg(desktop)]
fn list_row(
    sql: &(impl bae_db::client::query::QueryOne + bae_db::client::query::QueryRows),
    id: ImportListId,
) -> Result<ListRow, DbError> {
    let (
        (tab, pending_filter, filter_text, order),
        (view_revision, selection_revision, opened_at),
        (narrows, filters_text, keeps_all),
    ) = sql
        .query_row(
            "SELECT tab, pending_filter, filter_text, list_order, view_revision, \
                    selection_revision, opened_at, narrows, pattern IS NOT NULL, keeps_all \
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
                    (row.get::<_, i64>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?),
                    (row.get::<_, bool>(7)?, row.get::<_, bool>(8)?, row.get::<_, bool>(9)?),
                ))
            },
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("import list {} is not open", id.0)))?;
    Ok(ListRow {
        id,
        view: ImportListView {
            tab: order::tab_of(&tab)?,
            filter_text,
            pending_filter: pending_filter_of(&pending_filter)?,
            order: order::order_of(&order)?,
        },
        view_revision: to_u64(view_revision, "a list's view revision")?,
        selection_revision: to_u64(selection_revision, "a list's selection revision")?,
        opened_at: to_u64(opened_at, "where a list's view opened")?,
        narrows,
        filters_text,
        keeps_all,
    })
}

#[cfg(test)]
#[path = "import_list_tests.rs"]
pub(crate) mod tests;
