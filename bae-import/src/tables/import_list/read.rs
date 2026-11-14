//! One read of a list: its counts, its windows, and what its selection holds,
//! each window read from its cursor when it has one; and the live query that
//! reruns the read when its request or a table it reads changes.

use super::order::WindowItems;
use super::*;
use crate::list::{
    ImportListNarrowing, ImportListProjection, ImportListRequest, ImportQueueSummary,
};

#[derive(Clone, PartialEq)]
struct ImportListRows {
    windows: Vec<bae_store::LibraryBrowseWindow<window::WindowItemRows>>,
    total_count: u64,
    summary: ImportQueueSummary,
    pending_filter_entries: Vec<crate::PendingFilterEntry>,
    selection: crate::selection::SelectionSummary,
    view_revision: u64,
    selection_revision: u64,
}

/// One read of the list for `request`: each window from its cursor when it
/// has one, else by its offset (`list_windows`); `None` once the list is
/// closed.
fn load_import_list_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<Option<ImportListRows>, DbError> {
    let Some(list) = list_row(sql, request.list)? else {
        return Ok(None);
    };
    let counts = summary::counts(sql)?;
    let listed = summary::listed(sql, &list, &counts)?;
    let windows = request
        .windows
        .iter()
        .map(|window| {
            let items = list.items(listed.shown(window));
            let WindowItems { keys, first, last } = items.window(sql, window)?;
            let headers = items.header_positions(sql, window.offset, &keys, first.as_ref())?;
            Ok(bae_store::LibraryBrowseWindow {
                window: window.clone(),
                rows: window::materialise(sql, &list, &keys, &headers)?,
                first,
                last,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let watched_folders = super::watched_folders::watched_folders_on(sql)?;
    let selection = selection::selection(sql, list.id)?;
    Ok(Some(ImportListRows {
        windows,
        total_count: listed.total,
        summary: ImportQueueSummary {
            watched_folders,
            has_groups: counts.has_groups,
            narrowed: listed.narrowed,
            narrowing: ImportListNarrowing {
                tab: list.view.tab,
                filter_text: list.view.filter_text.clone(),
                pending_filter: list.view.pending_filter,
            },
            opened_at: list.opened_at,
        },
        pending_filter_entries: counts.entries,
        selection,
        view_revision: list.view_revision,
        selection_revision: list.selection_revision,
    }))
}

impl ImportListRows {
    fn process(self) -> Result<ImportListProjection, DbError> {
        let windows = self
            .windows
            .into_iter()
            .map(|window| window.try_map_rows(window::WindowItemRows::process))
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

impl ImportTables {
    /// The list a request names as a live query whose request carries the
    /// windows, so changing them reruns the read on the same subscription.
    /// Everything else the read shows is in the tables, so a write to the
    /// list's view, folds or selection reruns it too.
    pub(crate) fn subscribe_import_list(
        &self,
        initial: ImportListRequest,
    ) -> coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection, ImportListError>
    {
        self.store
            .subscribe_reconfigurable(initial, move |request, sql| {
                load_import_list_on(&sql, request)?.ok_or(ImportListError::Closed(request.list))
            })
            .process(|rows| Ok(rows.process()?))
    }

    /// One read of the list, for a caller with no subscription.
    pub async fn load_import_list(
        &self,
        request: ImportListRequest,
    ) -> Result<ImportListProjection, ImportListError> {
        let list = request.list;
        self.store
            .read(move |sql| load_import_list_on(&sql, &request))
            .process(move |rows| Ok(rows.ok_or(ImportListError::Closed(list))?.process()?))
            .await
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
