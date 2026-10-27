//! The writes a surface makes to the list it has open: opening and closing
//! it, changing its view, folding its groups and changing its selection. Each
//! is one write the list's live query answers, and each answers with the
//! revision the read that reflects it carries.

use super::order::{order_column, tab_column, Item};
use super::*;
use crate::import::selection::SelectionChange;
use crate::import::FolderReleaseDecisionKey;
use crate::library::LibraryPageWindow;

/// What a change to a selection came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionWrite {
    /// The selection is as asked, at this revision.
    Changed { revision: u64 },
    /// A run of rows was asked for between rows the list does not show, so
    /// nothing changed.
    RunNotShown,
}

fn view_revision(sql: &SqlContext<'_, '_>, list: ImportListId) -> Result<u64, DbError> {
    let revision = sql
        .query_row(
            "SELECT view_revision FROM import_list WHERE id = ?",
            [list.0],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("import list {} is not open", list.0)))?;
    to_u64(revision, "a list's view revision")
}

fn selection_revision(sql: &SqlContext<'_, '_>, list: ImportListId) -> Result<u64, DbError> {
    let revision = sql
        .query_row(
            "SELECT selection_revision FROM import_list WHERE id = ?",
            [list.0],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("import list {} is not open", list.0)))?;
    to_u64(revision, "a list's selection revision")
}

/// The view moved: a fold folded or opened something.
fn view_moved(sql: &SqlContext<'_, '_>, list: ImportListId) -> Result<(), DbError> {
    sql.execute(
        "UPDATE import_list SET view_revision = view_revision + 1 WHERE id = ?",
        [list.0],
    )?;
    Ok(())
}

/// Insert every key of `keys` the list shows into its selection.
fn select_shown(
    sql: &SqlContext<'_, '_>,
    list: ImportListId,
    keys: &[String],
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_list_selection (list_id, key) \
         SELECT shown.list_id, shown.key FROM json_each(:keys) AS w \
         JOIN import_list_shown AS shown ON shown.key = w.value \
         WHERE shown.list_id = :list \
         ON CONFLICT (list_id, key) DO NOTHING",
        named_params! {
            ":keys": serde_json::Value::from(keys.to_vec()).to_string(),
            ":list": list.0,
        },
    )?;
    Ok(())
}

impl Database {
    /// Open a list showing `view`, with nothing folded and nothing selected.
    pub(crate) async fn open_import_list(
        &self,
        view: ImportListView,
    ) -> Result<ImportListId, DbError> {
        self.call(move |sql| {
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
        self.call(move |sql| {
            sql.execute("DELETE FROM import_list WHERE id = ?", [list.0])?;
            Ok(())
        })
        .await
    }

    /// Close every list: what the library opens with, since no list outlives
    /// the process that opened it.
    pub(crate) async fn close_import_lists(&self) -> Result<(), DbError> {
        self.call(|sql| {
            sql.execute("DELETE FROM import_list", [])?;
            Ok(())
        })
        .await
    }

    /// Show `view` in `list`, and answer with the view revision the read
    /// that shows it carries. The rows it stops showing leave the selection
    /// in the same write. The view it already shows changes nothing.
    pub(crate) async fn set_import_list_view(
        &self,
        list: ImportListId,
        view: ImportListView,
    ) -> Result<u64, DbError> {
        self.call(move |sql| {
            sql.execute(
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
        self.call(move |sql| {
            let changed = match folded {
                true => sql.execute(
                    "INSERT INTO import_list_collapsed (list_id, watched_folder_path, group_name) \
                     SELECT ?, watched_folder_path, key FROM import_list_placement \
                     WHERE watched_folder_path = ? AND item = 'group' AND key = ? \
                     ON CONFLICT (list_id, watched_folder_path, group_name) DO NOTHING",
                    params![
                        list.0,
                        group.watched_folder_path,
                        group.relative_folder_path
                    ],
                )?,
                false => sql.execute(
                    "DELETE FROM import_list_collapsed \
                     WHERE list_id = ? AND watched_folder_path = ? AND group_name = ?",
                    params![
                        list.0,
                        group.watched_folder_path,
                        group.relative_folder_path
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
        self.call(move |sql| {
            let changed = match folded {
                true => sql.execute(
                    "INSERT INTO import_list_collapsed (list_id, watched_folder_path, group_name) \
                     SELECT ?, watched_folder_path, key FROM import_list_placement \
                     WHERE item = 'group' \
                     ON CONFLICT (list_id, watched_folder_path, group_name) DO NOTHING",
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

    /// Apply a change a person made by pointing at rows of `list`. A row the
    /// list does not show is never selected.
    pub(crate) async fn change_import_list_selection(
        &self,
        list: ImportListId,
        change: SelectionChange,
    ) -> Result<SelectionWrite, DbError> {
        self.call(move |sql| {
            match change {
                SelectionChange::Replace { keys } => {
                    sql.execute(
                        "DELETE FROM import_list_selection \
                         WHERE list_id = ? AND key NOT IN (SELECT value FROM json_each(?))",
                        params![list.0, serde_json::Value::from(keys.clone()).to_string()],
                    )?;
                    select_shown(sql, list, &keys)?;
                }
                SelectionChange::Toggle { add, remove } => {
                    sql.execute(
                        "DELETE FROM import_list_selection \
                         WHERE list_id = ? AND key IN (SELECT value FROM json_each(?))",
                        params![list.0, serde_json::Value::from(remove).to_string()],
                    )?;
                    select_shown(sql, list, &add)?;
                }
                SelectionChange::Extend { from, to } => {
                    let row = list_row(sql, list)?;
                    let items = row.items(Shown::View);
                    let (Some(from), Some(to)) =
                        (items.position_of(sql, &from)?, items.position_of(sql, &to)?)
                    else {
                        return Ok(SelectionWrite::RunNotShown);
                    };
                    let run = items.window(
                        sql,
                        &LibraryPageWindow {
                            offset: from.min(to),
                            limit: from.abs_diff(to) + 1,
                        },
                    )?;
                    let keys: Vec<String> = run
                        .into_iter()
                        .filter(|item| item.item != Item::Group)
                        .map(|item| item.key)
                        .collect();
                    select_shown(sql, list, &keys)?;
                }
            }
            Ok(SelectionWrite::Changed {
                revision: selection_revision(sql, list)?,
            })
        })
        .await
    }

    /// Select every row `list` shows, and answer with the selection revision
    /// the read that shows it carries.
    pub(crate) async fn select_all_in_import_list(
        &self,
        list: ImportListId,
    ) -> Result<u64, DbError> {
        self.call(move |sql| {
            sql.execute(
                "INSERT INTO import_list_selection (list_id, key) \
                 SELECT list_id, key FROM import_list_shown WHERE list_id = ? \
                 ON CONFLICT (list_id, key) DO NOTHING",
                [list.0],
            )?;
            selection_revision(sql, list)
        })
        .await
    }
}
