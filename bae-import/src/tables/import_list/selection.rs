//! A list's selection (`import_list_selection`): the rows a person points
//! at, which are only ever rows the list shows. What it holds and offers,
//! read beside the list's windows; the selected rows a bulk action takes, in
//! the list's order; and the writes that change it, each answering with the
//! selection revision the read that reflects it carries.

use super::listed::{BasisColumns, BASIS_COLUMNS};
use super::order::{sorted_terms, unit_join, uploading_cte, Item};
use super::*;
use crate::selection::{
    SelectedCandidate, SelectedRows, SelectionChange, SelectionSummary, SingleSelection,
};
use crate::tables::import_state::{RuntimeFactColumns, RUNTIME_FACT_COLUMNS};
use crate::triage::{selection_offers, SelectionPart};
use crate::CandidateKey;
use bae_store::query::{QueryOne, QueryRows};

/// What the list's selection holds and offers. The offers are worked out
/// once for each set of selected rows alike in everything their commands are
/// decided from, weighted by how many rows it holds, rather than once a row.
pub(super) fn selection(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<SelectionSummary, DbError> {
    let groups = sql.query(
        &format!(
            "SELECT p.item, {BASIS_COLUMNS}, {RUNTIME_FACT_COLUMNS}, count(*), MIN(p.key) \
             FROM import_list_selection AS s \
             JOIN import_list_placement AS p ON p.key = s.key AND p.item <> 'group' \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
             WHERE s.list_id = ? \
             GROUP BY p.item, {BASIS_COLUMNS}, {RUNTIME_FACT_COLUMNS}"
        ),
        [list.0],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                BasisColumns::read(row, 1)?,
                RuntimeFactColumns::read(row, 7)?,
                row.get::<_, i64>(11)?,
                row.get::<_, CandidateKey>(12)?,
            ))
        },
    )?;
    let mut parts = Vec::with_capacity(groups.len());
    let mut count = 0;
    let mut single = None;
    for (item, basis, facts, rows, key) in groups {
        let rows = to_u64(rows, "a selection's count")?;
        count += rows;
        let item = Item::of(&item)?;
        single = Some((item, key.clone()));
        // A folder that failed validation takes part in the selection — it
        // holds back combining it — with no action of its own.
        let actions = match item {
            Item::Candidate => basis.basis(&key)?.actions(&facts.facts(&key)?),
            Item::Invalid | Item::Group => Vec::new(),
        };
        parts.push(SelectionPart { actions, count: rows });
    }
    let single = match (count, single) {
        (1, Some((Item::Candidate, key))) => Some(SingleSelection::Candidate { key }),
        (1, Some((Item::Invalid, key))) => Some(SingleSelection::InvalidCandidate { key }),
        _ => None,
    };
    Ok(SelectionSummary { count, single, offers: selection_offers(&parts) })
}

/// The selected rows of the list, in its order.
pub(super) fn selected(
    sql: &(impl QueryOne + QueryRows),
    list: ImportListId,
) -> Result<SelectedRows, DbError> {
    let row = super::list_row(sql, list)?;
    let rows = sql.query(
        &format!(
            "WITH {uploading} \
             SELECT p.item, p.key, c.name, {BASIS_COLUMNS}, {RUNTIME_FACT_COLUMNS} \
             FROM import_list_selection AS s \
             JOIN import_list_placement AS p ON p.key = s.key AND p.item <> 'group' \
             {unit} \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
             LEFT JOIN uploading AS u ON u.key = p.key \
             WHERE s.list_id = ? \
             ORDER BY u.position IS NULL, u.position, {terms}",
            uploading = uploading_cte(),
            unit = unit_join("p", "unit"),
            terms = sorted_terms(row.view.order, "p", "unit"),
        ),
        [list.0],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, CandidateKey>(1)?,
                row.get::<_, String>(2)?,
                BasisColumns::read(row, 3)?,
                RuntimeFactColumns::read(row, 9)?,
            ))
        },
    )?;
    let mut selected = SelectedRows::default();
    for (item, key, name, basis, facts) in rows {
        match Item::of(&item)? {
            Item::Candidate => selected.candidates.push(SelectedCandidate {
                basis: basis.basis(&key)?,
                facts: facts.facts(&key)?,
                candidate_key: key,
                name,
            }),
            Item::Invalid | Item::Group => selected.invalid.push(key),
        }
    }
    Ok(selected)
}

impl ImportTables {
    /// The rows the list `list` has selected, in the order it shows them:
    /// what a bulk action runs over.
    pub async fn load_import_list_selected(
        &self,
        list: ImportListId,
    ) -> Result<crate::selection::SelectedRows, DbError> {
        self.store.read(move |sql| selected(&sql, list)).await
    }

    /// Apply a change a person made by pointing at rows of `list`. A row the
    /// list does not show is never selected.
    pub async fn change_import_list_selection(
        &self,
        list: ImportListId,
        change: SelectionChange,
    ) -> Result<SelectionWrite, DbError> {
        self.store
            .call(move |sql| {
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
                        let row = list_row(sql, list)?;
                        let items = row.items(Shown::View);
                        let (Some(from), Some(to)) =
                            (items.position_of(sql, &from)?, items.position_of(sql, &to)?)
                        else {
                            return Ok(SelectionWrite::RunNotShown);
                        };
                        items.select_window(
                            sql,
                            &LibraryPageWindow {
                                offset: from.min(to),
                                limit: from.abs_diff(to) + 1,
                            },
                        )?;
                    }
                }
                Ok(SelectionWrite::Changed { revision: selection_revision(sql, list)? })
            })
            .await
    }

    /// Select every row `list` shows, and answer with the selection revision
    /// the read that shows it carries.
    pub(crate) async fn select_all_in_import_list(
        &self,
        list: ImportListId,
    ) -> Result<u64, DbError> {
        self.store
            .call(move |sql| {
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

/// What a change to a selection came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionWrite {
    /// The selection is as asked, at this revision.
    Changed { revision: u64 },
    /// A run of rows was asked for between rows the list does not show, so
    /// nothing changed.
    RunNotShown,
}

fn selection_revision(sql: &CovenWrite<'_>, list: ImportListId) -> Result<u64, DbError> {
    let revision = sql
        .query_row("SELECT selection_revision FROM import_list WHERE id = ?", [list.0], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .ok_or_else(|| DbError::Message(format!("import list {} is not open", list.0)))?;
    to_u64(revision, "a list's selection revision")
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
