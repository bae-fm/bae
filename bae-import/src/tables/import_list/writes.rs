//! The list a surface has open (`import_list`) and the groups it folds
//! (`import_list_collapsed`): opening and closing it, changing its view and
//! folding its groups. Each is one write the list's live queries answer; a
//! view change answers where the new view opens, and a fold answers nothing.

use super::order::Shown;
use super::views::{order_column, tab_column};
use super::*;
use crate::{FolderReleaseDecisionKey, ImportListOpening};

/// Where the list's view opens: its first selected row, as the write found
/// it, or its top with no selected row shown. The write found the list open.
fn opening(sql: &CovenWrite<'_>, list: ImportListId) -> Result<ImportListOpening, DbError> {
    let row = list_row(sql, list)?.expect("the write found the list open");
    Ok(match row.items(Shown::View).position_of_first_selected(sql)? {
        Some((position, key)) => ImportListOpening::At { key, position },
        None => ImportListOpening::Top,
    })
}

impl ImportTables {
    /// Open a list showing `view`, with nothing folded and nothing selected.
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

    /// Close a list: its row, its folds and its selection go.
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

    /// Show `view` in `list`, and answer with where the new view opens. The
    /// rows it stops showing leave the selection in the same write, and the
    /// view opens at its first selected row. The view it already shows
    /// changes nothing and answers nothing.
    pub async fn set_import_list_view(
        &self,
        list: ImportListId,
        view: ImportListView,
    ) -> Result<Option<ImportListOpening>, ImportListError> {
        self.store
            .call(move |sql| {
                if list_row(sql, list)?.is_none() {
                    return Ok(None);
                }
                let changed = sql.execute(
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
                Ok(Some(match changed > 0 {
                    true => Some(opening(sql, list)?),
                    false => None,
                }))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// Fold the group `group` shut in `list`, or open it. A folded group's
    /// selected releases leave the selection. A group the list no longer has
    /// is no group to fold.
    pub(crate) async fn set_import_list_group_folded(
        &self,
        list: ImportListId,
        group: FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<(), ImportListError> {
        self.store.call(move |sql| {
            if list_row(sql, list)?.is_none() {
                return Ok(None);
            }
            match folded {
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
            Ok(Some(()))
        })
        .await?
        .ok_or(ImportListError::Closed(list))
    }

    /// Fold every group of `list` shut, or open them all, in one write.
    pub(crate) async fn set_import_list_groups_folded(
        &self,
        list: ImportListId,
        folded: bool,
    ) -> Result<(), ImportListError> {
        self.store.call(move |sql| {
            if list_row(sql, list)?.is_none() {
                return Ok(None);
            }
            match folded {
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
            Ok(Some(()))
        })
        .await?
        .ok_or(ImportListError::Closed(list))
    }
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
