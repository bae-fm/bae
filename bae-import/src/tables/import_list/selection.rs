//! A list's selection, which the surface keeps as the keys of the rows the
//! person selected: what those rows hold and offer, read as a live query of
//! its own whatever the list shows; the ones the list shows, in its order,
//! which a bulk action takes; and the keys a Shift run and Select All select.

use super::actions::{action_columns, offers};
use super::order::{sorted_terms, unit_join, uploading_cte, Item};
use super::*;
use crate::selection::{SelectedCandidate, SelectedRows, SelectionSummary, SingleSelection};
use crate::triage::SelectionOffer;
use crate::{CandidateAction, CandidateKey};

/// What the rows at `keys` hold and offer, in one statement, whatever the
/// list shows: how many rows the keys name, the one row when they name one,
/// and for each command how many of the rows offer it and whether it can run
/// as the selection stands. A key whose row the tables no longer hold names
/// no row. The rows are the summary (position 0) and then each command
/// offered, by its place in [`CandidateAction::ALL`]. Each command runs row
/// by row over the rows that offer it, so the selection offers it when any
/// row does. Combine is one command over the whole selection: the selection
/// offers Combine for two or more rows and enables it, counting every row,
/// only when Combine can merge every row; otherwise it counts none. A row that
/// Combine cannot merge shows the reason, and Combine stays in the offers.
pub(super) fn selection(
    sql: &SqlReadContext<'_>,
    keys: &[CandidateKey],
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
                "SELECT {at} + 1, \
                        CASE WHEN COALESCE(every_can_combine, 0) THEN n_rows ELSE 0 END, \
                        COALESCE(every_can_combine, 0), NULL, NULL \
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
                 FROM json_each(?) AS asked \
                 JOIN import_list_placement AS p \
                     ON p.key = unhex(asked.value) AND p.item <> 'group' \
                 LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key), \
             total AS ( \
                 SELECT COUNT(*) AS n_rows, MIN(item) AS item, MIN(key) AS key, \
                        MIN(can_Combine) AS every_can_combine, {sums} \
                 FROM chosen) \
             SELECT 0, n_rows, 1, CASE WHEN n_rows = 1 THEN item END, \
                    CASE WHEN n_rows = 1 THEN key END FROM total \
             UNION ALL {offers} ORDER BY 1",
            actions = action_columns(),
        ),
        [crate::tables::key_hex_list(keys.iter())],
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

/// The rows at `keys` the list `list` shows, in its order, and, when the
/// caller names a command in `acting`, only the rows that offer it; `None`
/// once the list is closed.
pub(super) fn selected(
    sql: &impl coven::SqlRead,
    list: ImportListId,
    keys: &[CandidateKey],
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
             FROM json_each(:keys) AS asked \
             JOIN import_list_shown AS shown \
                 ON shown.list_id = :list AND shown.key = unhex(asked.value) \
             JOIN import_list_placement AS p ON p.key = shown.key AND p.item <> 'group' \
             {unit} \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
             LEFT JOIN uploading AS u ON u.key = p.key \
             WHERE {offering} \
             ORDER BY u.position IS NULL, u.position, {terms}",
            uploading = uploading_cte(),
            unit = unit_join("p", "unit"),
            terms = sorted_terms(row.view.order, "p", "unit"),
        ),
        named_params! {
            ":keys": crate::tables::key_hex_list(keys.iter()),
            ":list": list.0,
        },
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

/// The keys of `keys` whose rows the list `list` shows.
pub(super) fn shown_among(
    sql: &impl coven::SqlRead,
    list: ImportListId,
    keys: &[CandidateKey],
) -> Result<Vec<CandidateKey>, DbError> {
    Ok(sql.query(
        "SELECT shown.key FROM json_each(:keys) AS asked \
         JOIN import_list_shown AS shown \
             ON shown.list_id = :list AND shown.key = unhex(asked.value)",
        named_params! {
            ":keys": crate::tables::key_hex_list(keys.iter()),
            ":list": list.0,
        },
        |row| row.get(0),
    )?)
}

/// The keys of `keys` whose rows the list `list` doesn't show.
pub(super) fn unshown_among(
    sql: &impl coven::SqlRead,
    list: ImportListId,
    keys: &[CandidateKey],
) -> Result<Vec<CandidateKey>, DbError> {
    Ok(sql.query(
        "SELECT unhex(asked.value) FROM json_each(:keys) AS asked \
         WHERE NOT EXISTS (SELECT 1 FROM import_list_shown AS shown \
                           WHERE shown.list_id = :list AND shown.key = unhex(asked.value))",
        named_params! {
            ":keys": crate::tables::key_hex_list(keys.iter()),
            ":list": list.0,
        },
        |row| row.get(0),
    )?)
}

impl ImportTables {
    /// The rows at `keys` the list `list` shows, in its order: what a bulk
    /// action runs over. When the caller names a command in `acting`, the
    /// method returns only the rows that offer it.
    pub async fn load_import_list_selected(
        &self,
        list: ImportListId,
        keys: Vec<CandidateKey>,
        acting: Option<CandidateAction>,
    ) -> Result<SelectedRows, ImportListError> {
        self.store
            .read(move |sql| selected(&sql, list, &keys, acting))
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// The keys of every release the list `list` shows, loaded by a surface
    /// or not: what Select All selects.
    pub async fn import_list_keys(
        &self,
        list: ImportListId,
    ) -> Result<Vec<CandidateKey>, ImportListError> {
        self.store
            .read(move |sql| {
                if list_row(&sql, list)?.is_none() {
                    return Ok(None);
                }
                Ok(Some(sql.query(
                    "SELECT key FROM import_list_shown WHERE list_id = ?",
                    [list.0],
                    |row| row.get(0),
                )?))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// The releases the list `list` shows from the release `from` to the
    /// release `to`, both included, in the list's order: what Shift-clicking
    /// a row selects. The surface passes the tab and order it shows the list
    /// in as `tab` and `order`. When the list shows only one of `from` and
    /// `to`, the method returns that release alone. When the list shows
    /// neither, or shows another tab, the method returns no keys.
    pub async fn import_list_keys_between(
        &self,
        list: ImportListId,
        tab: crate::TriageTab,
        order: crate::ImportListOrder,
        from: CandidateKey,
        to: CandidateKey,
    ) -> Result<Vec<CandidateKey>, ImportListError> {
        self.store
            .read(move |sql| {
                if list_row(&sql, list)?.is_none() {
                    return Ok(None);
                }
                Ok(Some(Items::new(list, tab, order, Shown::View).keys_between(&sql, &from, &to)?))
            })
            .await?
            .ok_or(ImportListError::Closed(list))
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
