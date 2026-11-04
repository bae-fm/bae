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

mod listed;
mod order;
mod pane;
mod read;
mod selection;
mod summary;
mod window;
mod writes;

use super::*;
use crate::list::{ImportListId, ImportListView};
use crate::PendingFilter;
use folder_scans::columns::{to_u32, to_u64};
use order::{Items, Shown};

pub use selection::SelectionWrite;

/// One list's row as its reads take it: its view, its revisions, and whether
/// its view narrows its tab or keeps every row of it.
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

/// The list `id`'s row. A list is read only while it is open, so a missing
/// row is a list closed under its own read.
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
