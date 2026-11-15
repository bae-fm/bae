//! A list's selection (`import_list_selection`): the rows a person points
//! at, which are only ever rows the list shows, bar a Pending list's release
//! whose import landed. What it holds and offers,
//! read as a live query of its own; the selected rows a bulk action takes, in
//! the list's order; and the writes that change it, which answer nothing: the
//! rows they changed rerun the windows holding them.

use super::actions::{action_columns, offers};
use super::order::{sorted_terms, unit_join, uploading_cte, Item};
use super::*;
use crate::selection::{
    SelectedCandidate, SelectedRows, SelectionChange, SelectionSummary, SingleSelection,
};
use crate::triage::SelectionOffer;
use crate::{CandidateAction, CandidateKey};

/// What the list's selection holds and offers, in one statement: how many
/// rows it holds, the one row when it holds one, and for each command how many
/// of the rows offer it and whether it can run as the selection stands. The
/// rows are the summary (position 0) and then each command offered, by its
/// place in [`CandidateAction::ALL`]. Each command runs row by row over the
/// rows that offer it, so it is offered when any does. Combining is one
/// command over the whole selection: it is offered once there are two rows or
/// more, and can run only when every one of them could be combined, so the
/// reason it cannot is on the row that holds it back rather than a missing
/// item.
pub(super) fn selection(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<SelectionSummary, DbError> {
    let sums = CandidateAction::ALL
        .iter()
        .map(|action| format!("COALESCE(SUM(can_{action:?}), 0) AS count_{action:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let offers = CandidateAction::ALL
        .iter()
        .enumerate()
        .map(|(at, action)| match action {
            CandidateAction::Combine => format!(
                "SELECT {at} + 1, n_rows, COALESCE(every_can_combine, 0), NULL, NULL \
                 FROM total WHERE n_rows >= 2"
            ),
            _ => format!(
                "SELECT {at} + 1, count_{action:?}, 1, NULL, NULL FROM total \
                 WHERE count_{action:?} > 0"
            ),
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let rows = sql.query(
        &format!(
            "WITH chosen AS ( \
                 SELECT p.item, p.key, {actions} \
                 FROM import_list_selection AS s \
                 JOIN import_list_placement AS p ON p.key = s.key AND p.item <> 'group' \
                 LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
                 WHERE s.list_id = ?), \
             total AS ( \
                 SELECT COUNT(*) AS n_rows, MIN(item) AS item, MIN(key) AS key, \
                        MIN(can_Combine) AS every_can_combine, {sums} \
                 FROM chosen) \
             SELECT 0, n_rows, 1, CASE WHEN n_rows = 1 THEN item END, \
                    CASE WHEN n_rows = 1 THEN key END FROM total \
             UNION ALL {offers} ORDER BY 1",
            actions = action_columns(),
        ),
        [list.0],
        |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<CandidateKey>>(4)?,
            ))
        },
    )?;
    let mut summary = SelectionSummary::default();
    for (position, count, enabled, item, key) in rows {
        let count = to_u64(count, "a selection's count")?;
        match position.checked_sub(1) {
            None => {
                summary.count = count;
                summary.single = match (item.as_deref().map(Item::of).transpose()?, key) {
                    (Some(Item::Candidate), Some(key)) => Some(SingleSelection::Candidate { key }),
                    (Some(Item::Invalid), Some(key)) => {
                        Some(SingleSelection::InvalidCandidate { key })
                    }
                    _ => None,
                };
            }
            Some(at) => summary.offers.push(SelectionOffer {
                action: CandidateAction::ALL[at],
                count,
                enabled,
            }),
        }
    }
    Ok(summary)
}

/// The selected rows of the list, in its order, and of them those that offer
/// `acting` when a command is named; `None` once the list is closed.
pub(super) fn selected(
    sql: &impl coven::SqlRead,
    list: ImportListId,
    acting: Option<CandidateAction>,
) -> Result<Option<SelectedRows>, DbError> {
    let Some(row) = super::list_row(sql, list)? else {
        return Ok(None);
    };
    // Combining acts on every selected row, whatever each offers.
    let offering = match acting {
        Some(CandidateAction::Combine) | None => "1".to_string(),
        Some(action) => format!("(p.item = 'candidate' AND {})", offers(action)),
    };
    let rows = sql.query(
        &format!(
            "WITH {uploading} \
             SELECT p.item, p.key, c.name \
             FROM import_list_selection AS s \
             JOIN import_list_placement AS p ON p.key = s.key AND p.item <> 'group' \
             {unit} \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
             LEFT JOIN uploading AS u ON u.key = p.key \
             WHERE s.list_id = ? AND {offering} \
             ORDER BY u.position IS NULL, u.position, {terms}",
            uploading = uploading_cte(),
            unit = unit_join("p", "unit"),
            terms = sorted_terms(row.view.order, "p", "unit"),
        ),
        [list.0],
        |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, CandidateKey>(1)?, row.get::<_, String>(2)?))
        },
    )?;
    let mut selected = SelectedRows::default();
    for (item, key, name) in rows {
        match Item::of(&item)? {
            Item::Candidate => {
                selected.candidates.push(SelectedCandidate { candidate_key: key, name })
            }
            Item::Invalid | Item::Group => selected.invalid.push(key),
        }
    }
    Ok(Some(selected))
}

impl ImportTables {
    /// The rows the list `list` has selected, in the order it shows them:
    /// what a bulk action runs over, narrowed to the rows offering `acting`
    /// when a command is named.
    pub async fn load_import_list_selected(
        &self,
        list: ImportListId,
        acting: Option<CandidateAction>,
    ) -> Result<crate::selection::SelectedRows, ImportListError> {
        self.store
            .read(move |sql| selected(&sql, list, acting))
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// Apply a change a person made by pointing at rows of `list`. A row the
    /// list does not show is never selected.
    pub async fn change_import_list_selection(
        &self,
        list: ImportListId,
        change: SelectionChange,
    ) -> Result<SelectionWrite, ImportListError> {
        self.store
            .call(move |sql| {
                let Some(row) = list_row(sql, list)? else {
                    return Ok(None);
                };
                // A release whose import landed while the list was on
                // Pending stays selected until the person selects again.
                let kept_unshown: bool = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM import_list_selection AS s \
                     WHERE s.list_id = ? AND NOT EXISTS ( \
                         SELECT 1 FROM import_list_shown AS shown \
                         WHERE shown.list_id = s.list_id AND shown.key = s.key))",
                    [list.0],
                    |row| row.get(0),
                )?;
                if kept_unshown {
                    sql.execute(
                        "DELETE FROM import_list_selection \
                         WHERE list_id = ? AND NOT EXISTS ( \
                             SELECT 1 FROM import_list_shown AS shown \
                             WHERE shown.list_id = import_list_selection.list_id \
                               AND shown.key = import_list_selection.key)",
                        [list.0],
                    )?;
                }
                match change {
                    SelectionChange::Replace { keys } => {
                        sql.execute(
                            "DELETE FROM import_list_selection \
                         WHERE list_id = ? AND key NOT IN (SELECT unhex(value) FROM json_each(?))",
                            params![list.0, crate::tables::key_hex_list(keys.iter())],
                        )?;
                        select_shown(sql, list, &keys)?;
                    }
                    SelectionChange::Toggle { add, remove } => {
                        sql.execute(
                            "DELETE FROM import_list_selection \
                         WHERE list_id = ? AND key IN (SELECT unhex(value) FROM json_each(?))",
                            params![list.0, crate::tables::key_hex_list(remove.iter())],
                        )?;
                        select_shown(sql, list, &add)?;
                    }
                    SelectionChange::Extend { from, to } => {
                        let items = row.items(Shown::View);
                        let (Some((from, _)), Some((to, _))) =
                            (items.position_of(sql, &from)?, items.position_of(sql, &to)?)
                        else {
                            return Ok(Some(SelectionWrite::RunNotShown));
                        };
                        items.select_window(sql, from.min(to), from.abs_diff(to) + 1)?;
                    }
                }
                Ok(Some(SelectionWrite::Changed))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// Select every row `list` shows.
    pub(crate) async fn select_all_in_import_list(
        &self,
        list: ImportListId,
    ) -> Result<(), ImportListError> {
        self.store
            .call(move |sql| {
                if list_row(sql, list)?.is_none() {
                    return Ok(None);
                }
                sql.execute(
                    "INSERT INTO import_list_selection (list_id, key) \
                 SELECT list_id, key FROM import_list_shown WHERE list_id = ? \
                 ON CONFLICT (list_id, key) DO NOTHING",
                    [list.0],
                )?;
                Ok(Some(()))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }
}

/// What a change to a selection came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionWrite {
    /// The selection is as asked.
    Changed,
    /// A run of rows was asked for between rows the list does not show, so
    /// nothing changed.
    RunNotShown,
}

/// Insert every key of `keys` the list shows into its selection.
fn select_shown(
    sql: &CovenWrite<'_>,
    list: ImportListId,
    keys: &[CandidateKey],
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_list_selection (list_id, key) \
         SELECT shown.list_id, shown.key FROM json_each(:keys) AS w \
         JOIN import_list_shown AS shown ON shown.key = unhex(w.value) \
         WHERE shown.list_id = :list \
         ON CONFLICT (list_id, key) DO NOTHING",
        named_params! {
            ":keys": crate::tables::key_hex_list(keys.iter()),
            ":list": list.0,
        },
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
