//! The lists a surface has open: each list's view, the groups it folds and
//! the rows it selects, and the views its reads take its rows from.

use super::*;
use crate::{ImportListOrder, TriageTab};

pub(super) const SQL: &str = include_str!("views.sql");

/// The tab as the placement's `tab` column holds it.
pub(super) fn tab_column(tab: TriageTab) -> &'static str {
    match tab {
        TriageTab::Pending => "pending",
        TriageTab::Done => "done",
        TriageTab::Skipped => "skipped",
    }
}

pub(super) fn tab_of(column: &str) -> Result<TriageTab, DbError> {
    match column {
        "pending" => Ok(TriageTab::Pending),
        "done" => Ok(TriageTab::Done),
        "skipped" => Ok(TriageTab::Skipped),
        other => Err(DbError::unreadable("an import list's stored tab", other)),
    }
}

/// The order as the list's `list_order` column holds it.
pub(super) fn order_column(order: ImportListOrder) -> &'static str {
    match order {
        ImportListOrder::NewestFirst => "newest_first",
        ImportListOrder::OldestFirst => "oldest_first",
        ImportListOrder::PathAscending => "path_ascending",
        ImportListOrder::PathDescending => "path_descending",
    }
}

pub(super) fn order_of(column: &str) -> Result<ImportListOrder, DbError> {
    match column {
        "newest_first" => Ok(ImportListOrder::NewestFirst),
        "oldest_first" => Ok(ImportListOrder::OldestFirst),
        "path_ascending" => Ok(ImportListOrder::PathAscending),
        "path_descending" => Ok(ImportListOrder::PathDescending),
        other => Err(DbError::unreadable("import_list.list_order", other)),
    }
}
