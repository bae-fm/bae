//! A list's selection, which the surface keeps as the keys of the rows the
//! person selected: what those rows hold and offer, read as a live query of
//! its own whatever the list shows; the ones the list shows, in its order,
//! which a bulk action takes; the keys a Shift run and Select All select; and
//! which selected rows another view or a fold takes out of the list.

use super::actions::{action_columns, offers};
use super::order::{sorted_terms, unit_join, uploading_cte, Item};
use super::*;
use crate::selection::{SelectedCandidate, SelectedRows, SelectionSummary, SingleSelection};
use crate::triage::SelectionOffer;
use crate::{CandidateAction, CandidateKey, ImportListOpening, ImportListViewChange};

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

/// The rows at `keys` the list `request` shows, in its order, and, when the
/// caller names a command in `acting`, only the rows that offer it.
pub(super) fn selected(
    sql: &impl coven::SqlRead,
    request: &ImportListRequest,
    keys: &[CandidateKey],
    acting: Option<CandidateAction>,
) -> Result<SelectedRows, DbError> {
    // Combining acts on every selected row, whatever each offers.
    let offering = match acting {
        Some(CandidateAction::Combine) | None => "1".to_string(),
        Some(action) => format!("(p.item = 'candidate' AND {})", offers(action)),
    };
    let keys = crate::tables::key_hex_list(keys.iter());
    let rows = Items::new(request.clone()).query(
        sql,
        &format!(
            "WITH {uploading} \
             SELECT p.item, p.key, c.name \
             FROM json_each(:keys) AS asked \
             JOIN {shown} AS shown ON shown.key = unhex(asked.value) \
             JOIN import_list_placement AS p ON p.key = shown.key AND p.item <> 'group' \
             {unit} \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
             LEFT JOIN uploading AS u ON u.key = p.key \
             WHERE {offering} \
             ORDER BY u.position IS NULL, u.position, {terms}",
            uploading = uploading_cte(),
            shown = request.shown(),
            unit = unit_join("p", "unit"),
            terms = sorted_terms(request.view.order, "p", "unit"),
        ),
        &[(":keys", &keys)],
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
    Ok(selected)
}

/// Where the list `request` shows opens: the position of the first of `keys`
/// it shows, or its top when it shows none; and the keys it doesn't show,
/// which leave the selection.
pub(super) fn view_change(
    sql: &impl coven::SqlRead,
    request: &ImportListRequest,
    keys: &[CandidateKey],
) -> Result<ImportListViewChange, DbError> {
    let items = Items::new(request.clone());
    let opening = match items.position_of_first(sql, keys)? {
        Some(position) => ImportListOpening::At { position },
        None => ImportListOpening::Top,
    };
    let left = items.query(
        sql,
        &format!(
            "SELECT unhex(asked.value) FROM json_each(:keys) AS asked \
             WHERE NOT EXISTS (SELECT 1 FROM {shown} AS shown \
                               WHERE shown.key = unhex(asked.value))",
            shown = request.shown(),
        ),
        &[(":keys", &crate::tables::key_hex_list(keys.iter()))],
        |row| row.get(0),
    )?;
    Ok(ImportListViewChange { opening, left })
}

/// The keys of `keys` whose rows the list `before` showed and the list
/// `after` doesn't: what a fold hid. The two requests differ in their folds
/// only, so the statement reads `before`'s from `:folded_before`.
pub(super) fn hidden_by(
    sql: &impl coven::SqlRead,
    before: &ImportListRequest,
    after: &ImportListRequest,
    keys: &[CandidateKey],
) -> Result<Vec<CandidateKey>, DbError> {
    let folded_before = before.folded_json();
    Items::new(after.clone()).query(
        sql,
        &format!(
            "SELECT unhex(asked.value) FROM json_each(:keys) AS asked \
             WHERE EXISTS (SELECT 1 FROM {before} AS shown \
                           WHERE shown.key = unhex(asked.value)) \
               AND NOT EXISTS (SELECT 1 FROM {after} AS shown \
                               WHERE shown.key = unhex(asked.value))",
            before = before.fragments_folding(":folded_before").shown(),
            after = after.shown(),
        ),
        &[(":keys", &crate::tables::key_hex_list(keys.iter())), (":folded_before", &folded_before)],
        |row| row.get(0),
    )
}

impl ImportTables {
    /// The rows at `keys` the list `request` shows, in its order: what a bulk
    /// action runs over. When the caller names a command in `acting`, the
    /// method returns only the rows that offer it.
    pub async fn load_import_list_selected(
        &self,
        request: ImportListRequest,
        keys: Vec<CandidateKey>,
        acting: Option<CandidateAction>,
    ) -> Result<SelectedRows, DbError> {
        self.store.read(move |sql| selected(&sql, &request, &keys, acting)).await
    }

    /// The keys of every release the list `request` shows, loaded by a
    /// surface or not: what Select All selects.
    pub async fn import_list_keys(
        &self,
        request: ImportListRequest,
    ) -> Result<Vec<CandidateKey>, DbError> {
        self.store
            .read(move |sql| {
                Items::new(request.clone()).query(
                    &sql,
                    &format!("SELECT key FROM {}", request.shown()),
                    &[],
                    |row| row.get(0),
                )
            })
            .await
    }

    /// The releases the list `request` shows from the release `from` to the
    /// release `to`, both included, in the list's order: what Shift-clicking
    /// a row selects. When the list shows only one of `from` and `to`, the
    /// method returns that release alone. When it shows neither, the method
    /// returns no keys.
    pub async fn import_list_keys_between(
        &self,
        request: ImportListRequest,
        from: CandidateKey,
        to: CandidateKey,
    ) -> Result<Vec<CandidateKey>, DbError> {
        self.store.read(move |sql| Items::new(request).keys_between(&sql, &from, &to)).await
    }

    /// Where the list `request` shows opens, and which of the selected keys
    /// it doesn't show.
    pub async fn import_list_view_change(
        &self,
        request: ImportListRequest,
        selected: Vec<CandidateKey>,
    ) -> Result<ImportListViewChange, DbError> {
        self.store.read(move |sql| view_change(&sql, &request, &selected)).await
    }

    /// The keys of `selected` that the list showed under `before` and doesn't
    /// under `after`, which differ in their folds only.
    pub async fn import_list_hidden_by(
        &self,
        before: ImportListRequest,
        after: ImportListRequest,
        selected: Vec<CandidateKey>,
    ) -> Result<Vec<CandidateKey>, DbError> {
        self.store.read(move |sql| hidden_by(&sql, &before, &after, &selected)).await
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
