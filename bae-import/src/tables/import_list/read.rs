//! One read of a list: its counts, its windows, and what its selection holds,
//! each window read from an item beside it that this read or, while the list
//! stands as it stood then, the last read found; and the live query that
//! reruns the read when its request or a table it reads changes.

use super::order::HeaderPlaces;
use super::*;
use crate::list::{
    ImportListNarrowing, ImportListProjection, ImportListRequest, ImportQueueSummary,
};

#[derive(Clone, PartialEq)]
struct ImportListRows {
    windows: Vec<(bae_store::LibraryPageWindow, Vec<window::WindowItemRows>)>,
    total_count: u64,
    summary: ImportQueueSummary,
    pending_filter_entries: Vec<crate::PendingFilterEntry>,
    selection: crate::selection::SelectionSummary,
    view_revision: u64,
    selection_revision: u64,
}

/// What places a list's rows: its view, and the tables its order and its
/// Done tab's uploads read, by their versions.
type Standing = (ImportListId, u64, Vec<i64>);

/// The tables whose rows place a list's items, as `import_list_versions`
/// names them: the placement, and for Done the releases still uploading,
/// found by the hash of the folder each came from, and their place in the
/// upload queue.
const LIST_SOURCES: &[&str] =
    &["import_list_placement", "local_release_origin", "release_storage", "scan_candidate"];

/// Where a read found a list's rows: each window's first and last items in
/// each part, and the headers of the groups its windows list.
#[derive(Debug, Clone, Default)]
struct ListPlaces {
    items: Places,
    headers: HeaderPlaces,
}

/// One read of the list for `request`, each window read from an item beside
/// it when this read or, while the list stands as it stood then, the last
/// read through `places` found one (`list_windows`); `None` once the list is
/// closed.
fn load_import_list_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
    places: &KnownPlaces<Standing, ListPlaces>,
) -> Result<Option<ImportListRows>, DbError> {
    let Some(list) = list_row(sql, request.list)? else {
        return Ok(None);
    };
    let versions = list_versions_on(sql, "import_list_versions", LIST_SOURCES)?;
    let standing = (list.id, list.view_revision, versions);
    let mut known = places.under(&standing);
    let mut found = ListPlaces::default();
    let counts = summary::counts(sql)?;
    let listed = summary::listed(sql, &list, &counts)?;
    let windows = request
        .windows
        .iter()
        .map(|window| {
            let items = list.items(listed.shown(window, &known.items));
            let (keys, read) = items.window(sql, window, &known.items)?;
            known.items.extend(read.clone());
            found.items.extend(read);
            let headers = items.header_positions(sql, window.offset, &keys, &mut known.headers)?;
            Ok((window.clone(), window::materialise(sql, &list, &keys, &headers)?))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    found.headers = known.headers;
    places.keep(standing, found);
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
            .map(|(window, items)| {
                Ok(bae_store::LibraryBrowseWindow {
                    window,
                    rows: items
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
        let places = KnownPlaces::new();
        self.store
            .subscribe_reconfigurable(initial, move |request, sql| {
                load_import_list_on(&sql, request, &places)?
                    .ok_or(ImportListError::Closed(request.list))
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
            .read(move |sql| load_import_list_on(&sql, &request, &KnownPlaces::new()))
            .process(move |rows| Ok(rows.ok_or(ImportListError::Closed(list))?.process()?))
            .await
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
