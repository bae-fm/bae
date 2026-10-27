//! The import tab's list, read from its placement.
//!
//! Each list a surface has open is a row of `import_list`, beside the groups
//! it folds (`import_list_collapsed`) and the rows it selects
//! (`import_list_selection`); every release's tab, Found state, group, dates,
//! shown text and command basis are in `import_list_placement`, which triggers
//! keep in the write that changes any table they come from. A read of the list
//! is then a handful of indexed statements: the requested windows in the
//! list's order (`order`), each window's rows by key (`window`), and the
//! counts and the selection's offers beside them (`summary`). Nothing is
//! placed, filtered, sorted or counted outside SQLite.

mod listed;
mod order;
mod pane;
mod summary;
mod window;
mod writes;

use super::*;
use crate::import::list::{
    ImportCandidateDetailProjection, ImportListId, ImportListNarrowing, ImportListProjection,
    ImportListRequest, ImportListView, ImportListWindow, ImportQueueSummary,
};
use crate::import::watched_folder::WatchedFolder;
use crate::import::{ImportedRelease, PendingFilter};
use folder_scans::columns::{to_u32, to_u64, unreadable};
use order::{Items, Shown};

pub(crate) use writes::SelectionWrite;

/// One list's row as its reads take it: its view, its revisions, and whether
/// its view narrows its tab or keeps every row of it.
#[derive(Debug, Clone, PartialEq)]
struct ListRow {
    id: ImportListId,
    view: ImportListView,
    view_revision: u64,
    selection_revision: u64,
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
        other => Err(unreadable("a Found filter entry", other)),
    }
}

/// The list `id`'s row. A list is read only while it is open, so a missing
/// row is a list closed under its own read.
fn list_row(
    sql: &(impl super::query::QueryOne + super::query::QueryRows),
    id: ImportListId,
) -> Result<ListRow, DbError> {
    let (
        (tab, pending_filter, filter_text, order),
        (view_revision, selection_revision),
        (narrows, filters_text, keeps_all),
    ) = sql
        .query_row(
            "SELECT tab, pending_filter, filter_text, list_order, view_revision, \
                    selection_revision, narrows, needle <> '', keeps_all \
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
                    (row.get::<_, i64>(4)?, row.get::<_, i64>(5)?),
                    (
                        row.get::<_, bool>(6)?,
                        row.get::<_, bool>(7)?,
                        row.get::<_, bool>(8)?,
                    ),
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
        narrows,
        filters_text,
        keeps_all,
    })
}

#[derive(Clone, PartialEq)]
struct ImportListRows {
    windows: Vec<(
        crate::library::LibraryPageWindow,
        Vec<window::WindowItemRows>,
    )>,
    total_count: u64,
    summary: ImportQueueSummary,
    pending_filter_entries: Vec<crate::import::PendingFilterEntry>,
    selection: crate::import::selection::SelectionSummary,
    view_revision: u64,
    selection_revision: u64,
}

/// One read of the list for `request`.
fn load_import_list_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<ImportListRows, DbError> {
    let list = list_row(sql, request.list)?;
    let items = list.items(Shown::View);
    let windows = request
        .windows
        .iter()
        .map(|window| {
            let keys = items.window(sql, window)?;
            Ok((window.clone(), window::materialise(sql, &list, &keys)?))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let counts = summary::counts(sql)?;
    let total_count = summary::total_count(sql, &list)?;
    let narrowed = match list.narrows {
        true => Some(summary::narrowed(sql, &list, &counts)?),
        false => None,
    };
    let watched_folders = sql
        .query(
            "SELECT path FROM watched_import_folders ORDER BY position",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(WatchedFolder::from_path)
        .collect();
    let first_selected_position = summary::first_selected_position(sql, &list)?;
    let selection = summary::selection(sql, list.id)?;
    Ok(ImportListRows {
        windows,
        total_count,
        summary: ImportQueueSummary {
            counts: counts.tabs,
            watched_folders,
            has_groups: counts.has_groups,
            narrowed,
            narrowing: ImportListNarrowing {
                tab: list.view.tab,
                filter_text: list.view.filter_text.clone(),
                pending_filter: list.view.pending_filter,
            },
            first_selected_position,
        },
        pending_filter_entries: counts.entries,
        selection,
        view_revision: list.view_revision,
        selection_revision: list.selection_revision,
    })
}

impl ImportListRows {
    fn process(self) -> Result<ImportListProjection, DbError> {
        let windows = self
            .windows
            .into_iter()
            .map(|(window, items)| {
                Ok(ImportListWindow {
                    window,
                    items: items
                        .into_iter()
                        .map(window::WindowItemRows::process)
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(ImportListProjection {
            total_count: self.total_count,
            windows,
            summary: self.summary,
            pending_filter_entries: self.pending_filter_entries,
            selection: self.selection,
            view_revision: self.view_revision,
            selection_revision: self.selection_revision,
        })
    }
}

impl Database {
    /// The list a request names as a live query whose request carries the
    /// windows, so changing them reruns the read on the same subscription.
    /// Everything else the read shows is in the tables, so a write to the
    /// list's view, folds or selection reruns it too.
    pub(crate) fn subscribe_import_list(
        &self,
        initial: ImportListRequest,
    ) -> coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |request, sql| {
                load_import_list_on(&sql, request).map_err(CovenError::from)
            })
            .process(|rows| rows.process().map_err(CovenError::from))
    }

    /// The plan each part of `list`'s windows is read by, for a test that
    /// holds every order to its index.
    #[cfg(test)]
    pub(crate) async fn import_list_window_plans(
        &self,
        list: ImportListId,
    ) -> Result<Vec<Vec<String>>, DbError> {
        self.read(move |sql| {
            let row = list_row(&sql, list)?;
            row.items(Shown::View).window_plans(&sql)
        })
        .await
    }

    /// One read of the list, for a caller with no subscription.
    pub(crate) async fn load_import_list(
        &self,
        request: ImportListRequest,
    ) -> Result<ImportListProjection, DbError> {
        self.read(move |sql| load_import_list_on(&sql, &request))
            .process(ImportListRows::process)
            .await
    }

    /// Where the candidate at `candidate_key` sits in the list `list` once
    /// revealed: on the tab its placement puts it on, with the filters
    /// cleared and its group opened, the list's other folds kept. `None` when
    /// the list lists no such candidate.
    pub(crate) async fn locate_import_candidate(
        &self,
        list: ImportListId,
        candidate_key: &str,
    ) -> Result<Option<crate::import::ImportCandidateListLocation>, DbError> {
        let candidate_key = candidate_key.to_string();
        self.read(move |sql| summary::locate(&sql, list, &candidate_key))
            .await
    }

    /// The first candidate in the list's order, every tab's rows unfiltered,
    /// whose identification is queued, running or having its answer written;
    /// `None` when none is.
    pub(crate) async fn first_identifying_import_candidate(
        &self,
        list: ImportListId,
    ) -> Result<Option<String>, DbError> {
        self.read(move |sql| summary::first_identifying(&sql, list))
            .await
    }

    /// The rows the list `list` has selected, in the order it shows them:
    /// what a bulk action runs over.
    pub(crate) async fn load_import_list_selected(
        &self,
        list: ImportListId,
    ) -> Result<crate::import::selection::SelectedRows, DbError> {
        self.read(move |sql| summary::selected(&sql, list)).await
    }

    /// The candidate the request names, as the pane reads it, live; `None` for
    /// no key or a key naming no release the list lists, which clears a
    /// selection.
    pub(crate) fn subscribe_import_candidate(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<ImportCandidateDetailProjection>>
    {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |key, sql| match key {
                None => Ok(None),
                Some(key) => pane::load_candidate_detail_on(&sql, key).map_err(CovenError::from),
            })
            .process(|process| {
                process
                    .map(pane::CandidateDetailRows::process)
                    .transpose()
                    .map_err(CovenError::from)
            })
    }

    /// The folder that failed validation at the key the read is set to, as
    /// the pane looking into it reads it, live; `None` for no key, or a key
    /// naming no such folder, which clears a selection.
    pub(crate) fn subscribe_invalid_candidate(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<crate::import::InvalidCandidateDetail>>
    {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |key, sql| match key {
                None => Ok(None),
                Some(key) => {
                    let stored =
                        folder_scans::load_item_by_key(&sql, key, folder_scans::RowSources::Any)
                            .map_err(CovenError::from)?;
                    Ok(match stored.map(|(_, stored)| stored.item) {
                        Some(crate::import::folder_scanner::ScanItem::Invalid(candidate)) => {
                            Some(crate::import::InvalidCandidateDetail::of(candidate))
                        }
                        _ => None,
                    })
                }
            })
    }

    /// One candidate as the pane reads it, once.
    pub(crate) async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
        let key = key.to_string();
        self.read(move |sql| pane::load_candidate_detail_on(&sql, &key))
            .process(|process| process.map(pane::CandidateDetailRows::process).transpose())
            .await
    }
}
