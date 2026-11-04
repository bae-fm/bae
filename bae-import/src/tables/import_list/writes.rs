//! The list a surface has open (`import_list`) and the groups it folds
//! (`import_list_collapsed`): opening and closing it, changing its view and
//! folding its groups. Each is one write the list's live query answers, and
//! each answers with the view revision the read that reflects it carries.

use super::order::{order_column, tab_column, Shown};
use super::*;
use crate::FolderReleaseDecisionKey;

fn view_revision(sql: &CovenWrite<'_>, list: ImportListId) -> Result<u64, DbError> {
    let revision = sql
        .query_row("SELECT view_revision FROM import_list WHERE id = ?", [list.0], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .ok_or_else(|| DbError::Message(format!("import list {} is not open", list.0)))?;
    to_u64(revision, "a list's view revision")
}

/// The view moved: a fold folded or opened something.
fn view_moved(sql: &CovenWrite<'_>, list: ImportListId) -> Result<(), DbError> {
    sql.execute("UPDATE import_list SET view_revision = view_revision + 1 WHERE id = ?", [list.0])?;
    Ok(())
}

/// Where the list's view opens: the position of its first selected row,
/// reading the items before it, or its top with no selected row shown.
fn opened_at(sql: &CovenWrite<'_>, list: ImportListId) -> Result<u64, DbError> {
    let row = list_row(sql, list)?;
    Ok(row
        .items(Shown::View)
        .position_where(
            sql,
            "s.item <> 'group' AND EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                                           WHERE chosen.list_id = :list AND chosen.key = s.key)",
            &[],
        )?
        .unwrap_or(0))
}

impl ImportTables {
    /// Open a list showing `view`, with nothing folded and nothing selected.
    pub async fn open_import_list(&self, view: ImportListView) -> Result<ImportListId, DbError> {
        self.database
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
        self.database
            .call(move |sql| {
                sql.execute("DELETE FROM import_list WHERE id = ?", [list.0])?;
                Ok(())
            })
            .await
    }

    /// Close every list: what the library opens with, since no list outlives
    /// the process that opened it.
    pub(crate) async fn close_import_lists(&self) -> Result<(), DbError> {
        self.database
            .call(|sql| {
                sql.execute("DELETE FROM import_list", [])?;
                Ok(())
            })
            .await
    }

    /// Show `view` in `list`, and answer with the view revision the read
    /// that shows it carries. The rows it stops showing leave the selection
    /// in the same write, and the view opens at its first selected row. The
    /// view it already shows changes nothing.
    pub async fn set_import_list_view(
        &self,
        list: ImportListId,
        view: ImportListView,
    ) -> Result<u64, DbError> {
        self.database
            .call(move |sql| {
                let changed = sql.execute(
                    "UPDATE import_list \
                 SET tab = :tab, pending_filter = :filter, filter_text = :text, \
                     list_order = :order, view_revision = view_revision + 1 \
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
                if changed > 0 {
                    let opened_at =
                        to_i64(opened_at(sql, list)?, "the place an import list opened at")?;
                    sql.execute(
                        "UPDATE import_list SET opened_at = ? WHERE id = ?",
                        params![opened_at, list.0],
                    )?;
                }
                view_revision(sql, list)
            })
            .await
    }

    /// Fold the group `group` shut in `list`, or open it, and answer with the
    /// view revision the read that shows it carries. A folded group's
    /// selected releases leave the selection. A group the list no longer has
    /// is no group to fold.
    pub(crate) async fn set_import_list_group_folded(
        &self,
        list: ImportListId,
        group: FolderReleaseDecisionKey,
        folded: bool,
    ) -> Result<u64, DbError> {
        self.database.call(move |sql| {
            let changed = match folded {
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
            if changed > 0 {
                view_moved(sql, list)?;
            }
            view_revision(sql, list)
        })
        .await
    }

    /// Fold every group of `list` shut, or open them all, in one write, and
    /// answer with the view revision the read that shows it carries.
    pub(crate) async fn set_import_list_groups_folded(
        &self,
        list: ImportListId,
        folded: bool,
    ) -> Result<u64, DbError> {
        self.database.call(move |sql| {
            let changed = match folded {
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
            if changed > 0 {
                view_moved(sql, list)?;
            }
            view_revision(sql, list)
        })
        .await
    }
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
