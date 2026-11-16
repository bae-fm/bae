//! The list a surface has open (`import_list`) and the groups it folds
//! (`import_list_collapsed`): opening and closing it, changing its view and
//! folding its groups. Each is one write the list's live queries answer. The
//! surface passes the keys of the rows it keeps selected: a view change with
//! another tab, filter or search answers where the new view opens and which
//! selected rows leave the selection, and a fold answers which selected rows
//! it hid.

use super::order::Shown;
use super::selection::{shown_among, unshown_among};
use super::views::{order_column, tab_column};
use super::*;
use crate::{CandidateKey, FolderReleaseDecisionKey, ImportListOpening, ImportListViewChange};

/// Where the list's view opens: the first row of `selected` it shows, as the
/// write found it, or its top when it shows none. The write found the list
/// open.
fn opening(
    sql: &CovenWrite<'_>,
    list: ImportListId,
    selected: &[CandidateKey],
) -> Result<ImportListOpening, DbError> {
    let row = list_row(sql, list)?.expect("the write found the list open");
    Ok(match row.items(Shown::View).position_of_first(sql, selected)? {
        Some((position, key)) => ImportListOpening::At { key, position },
        None => ImportListOpening::Top,
    })
}

/// Run `fold` on `list`, and answer with the keys of `selected` whose rows
/// the list showed before it and doesn't after it.
fn hidden_by(
    sql: &CovenWrite<'_>,
    list: ImportListId,
    selected: &[CandidateKey],
    fold: impl FnOnce() -> Result<(), DbError>,
) -> Result<Vec<CandidateKey>, DbError> {
    let before = shown_among(sql, list, selected)?;
    fold()?;
    unshown_among(sql, list, &before)
}

impl ImportTables {
    /// Open a list showing `view`, with nothing folded.
    pub async fn open_import_list(&self, view: ImportListView) -> Result<ImportListId, DbError> {
        self.store
            .call(move |sql| {
                Ok(ImportListId(sql.query_row(
                    "INSERT INTO import_list (tab, pending_filter, filter_text, list_order) \
                 VALUES (?, ?, ?, ?) RETURNING id",
                    params![
                        tab_column(view.tab),
                        pending_filter_column(view.pending_filter),
                        view.filter_text,
                        order_column(view.order),
                    ],
                    |row| row.get::<_, i64>(0),
                )?))
            })
            .await
    }

    /// Close a list: its row and its folds go.
    pub(crate) async fn close_import_list(&self, list: ImportListId) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                sql.execute("DELETE FROM import_list WHERE id = ?", [list.0])?;
                Ok(())
            })
            .await
    }

    /// Close every list: what the library opens with, since no list outlives
    /// the process that opened it.
    pub(crate) async fn close_import_lists(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM import_list", [])?;
                Ok(())
            })
            .await
    }

    /// Show `view` in `list`, whose surface keeps the rows at `selected`
    /// selected. When the new view has another tab, filter or search, the
    /// write answers where it opens, at the first of those rows it shows or
    /// its top, and which of them leave the selection: each one the new view
    /// doesn't show. For another order, and for the view the list already
    /// shows, the write answers nothing: the table keeps its scroll while it
    /// draws the rows in the new order.
    pub async fn set_import_list_view(
        &self,
        list: ImportListId,
        view: ImportListView,
        selected: Vec<CandidateKey>,
    ) -> Result<Option<ImportListViewChange>, ImportListError> {
        self.store
            .call(move |sql| {
                let Some(row) = list_row(sql, list)? else {
                    return Ok(None);
                };
                let narrowed = (row.view.tab, row.view.pending_filter, &row.view.filter_text)
                    != (view.tab, view.pending_filter, &view.filter_text);
                sql.execute(
                    "UPDATE import_list \
                 SET tab = :tab, pending_filter = :filter, filter_text = :text, \
                     list_order = :order \
                 WHERE id = :list \
                   AND (tab, pending_filter, filter_text, list_order) \
                       IS NOT (:tab, :filter, :text, :order)",
                    named_params! {
                        ":list": list.0,
                        ":tab": tab_column(view.tab),
                        ":filter": pending_filter_column(view.pending_filter),
                        ":text": view.filter_text,
                        ":order": order_column(view.order),
                    },
                )?;
                if !narrowed {
                    return Ok(Some(None));
                }
                let opening = opening(sql, list, &selected)?;
                let left = unshown_among(sql, list, &selected)?;
                Ok(Some(Some(ImportListViewChange { opening, left })))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// Fold the group `group` shut in `list`, or open it, and answer with the
    /// keys of `selected`, the rows the surface keeps selected, whose rows
    /// the fold hid. A group the list no longer has is no group to fold.
    pub(crate) async fn set_import_list_group_folded(
        &self,
        list: ImportListId,
        group: FolderReleaseDecisionKey,
        folded: bool,
        selected: Vec<CandidateKey>,
    ) -> Result<Vec<CandidateKey>, ImportListError> {
        self.store.call(move |sql| {
            if list_row(sql, list)?.is_none() {
                return Ok(None);
            }
            Ok(Some(hidden_by(sql, list, &selected, || { match folded {
                true => sql.execute(
                    "INSERT INTO import_list_collapsed (list_id, watched_folder_path, group_folder) \
                     SELECT ?, watched_folder_path, key FROM import_list_placement \
                     WHERE watched_folder_path = ? AND item = 'group' AND key = ? \
                     ON CONFLICT (list_id, watched_folder_path, group_folder) DO NOTHING",
                    params![
                        list.0,
                        coven::path_bytes(&group.watched_folder_path),
                        group.relative_folder
                    ],
                )?,
                false => sql.execute(
                    "DELETE FROM import_list_collapsed \
                     WHERE list_id = ? AND watched_folder_path = ? AND group_folder = ?",
                    params![
                        list.0,
                        coven::path_bytes(&group.watched_folder_path),
                        group.relative_folder
                    ],
                )?,
            };
            Ok(()) })?))
        })
        .await?
        .ok_or(ImportListError::Closed(list))
    }

    /// Fold every group of `list` shut, or open them all, in one write, and
    /// answer with the keys of `selected`, the rows the surface keeps
    /// selected, whose rows the fold hid.
    pub(crate) async fn set_import_list_groups_folded(
        &self,
        list: ImportListId,
        folded: bool,
        selected: Vec<CandidateKey>,
    ) -> Result<Vec<CandidateKey>, ImportListError> {
        self.store.call(move |sql| {
            if list_row(sql, list)?.is_none() {
                return Ok(None);
            }
            Ok(Some(hidden_by(sql, list, &selected, || { match folded {
                true => sql.execute(
                    "INSERT INTO import_list_collapsed (list_id, watched_folder_path, group_folder) \
                     SELECT ?, watched_folder_path, key FROM import_list_placement \
                     WHERE item = 'group' \
                     ON CONFLICT (list_id, watched_folder_path, group_folder) DO NOTHING",
                    [list.0],
                )?,
                false => sql.execute(
                    "DELETE FROM import_list_collapsed WHERE list_id = ?",
                    [list.0],
                )?,
            };
            Ok(()) })?))
        })
        .await?
        .ok_or(ImportListError::Closed(list))
    }
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
