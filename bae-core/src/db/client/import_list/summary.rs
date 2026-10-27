//! What the chrome around a list says, read beside its windows: the tab
//! counts and Found's filter entries from the placement's counts index, how
//! many items the list holds and how many its filters keep, where its first
//! selected row sits, and what its selection holds and offers. And the reads a
//! surface makes of a list on its own: where a candidate would be revealed,
//! the first one being identified, and the selected rows a bulk action takes.

use super::super::import_state::{RuntimeFactColumns, RUNTIME_FACT_COLUMNS};
use super::super::query::{QueryOne, QueryRows};
use super::listed::{BasisColumns, BASIS_COLUMNS};
use super::order::{order_terms, tab_of, uploading_cte, Item};
use super::*;
use crate::import::list::{ImportCandidateListLocation, NarrowedCount};
use crate::import::selection::{
    SelectedCandidate, SelectedRows, SelectionSummary, SingleSelection,
};
use crate::import::triage::{selection_offers, SelectionPart};
use crate::import::{PendingFilterEntry, TriageTabCounts};

/// Every tab's count, how many of Found's releases each filter entry holds,
/// and whether Found has any group.
pub(super) struct Counts {
    pub(super) tabs: TriageTabCounts,
    pub(super) entries: Vec<PendingFilterEntry>,
    pub(super) has_groups: bool,
}

/// The counts kept beside the placement.
pub(super) fn counts(sql: &SqlReadContext<'_>) -> Result<Counts, DbError> {
    let mut tabs = TriageTabCounts::default();
    let mut has_groups = false;
    for (tab, item, count) in sql.query(
        "SELECT tab, item, count FROM import_list_tab_count",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )? {
        match Item::of(&item)? {
            Item::Group => has_groups = true,
            Item::Candidate | Item::Invalid => {
                tabs.add(tab_of(&tab)?, to_u32(count, "a tab's count")?)
            }
        }
    }
    let in_entries = sql
        .query(
            "SELECT pending_entry, count FROM import_list_entry_count",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?
        .into_iter()
        .map(|(entry, count)| {
            Ok((
                pending_filter_of(&entry)?,
                to_u32(count, "a filter entry's count")?,
            ))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(Counts {
        tabs,
        entries: crate::import::list::pending_filter_entries(&in_entries),
        has_groups,
    })
}

/// How many items the list holds: its releases its filters keep outside the
/// groups it folds shut, and the header of each group holding one. Without
/// a text filter that is the kept counts; a text filter is counted over the
/// rows it reads.
pub(super) fn total_count(sql: &SqlReadContext<'_>, list: &ListRow) -> Result<u64, DbError> {
    let statement = match (list.keeps_all, list.filters_text) {
        (true, _) => {
            "SELECT COALESCE(SUM(t.count), 0) FROM import_list_view AS v \
             JOIN import_list_tab_count AS t ON t.tab = v.tab WHERE v.id = ?"
        }
        (false, false) => {
            "SELECT \
                 CASE WHEN v.tab = 'pending' AND v.pending_filter <> 'all' \
                      THEN COALESCE((SELECT e.count FROM import_list_entry_count AS e \
                                     WHERE e.pending_entry = v.pending_filter), 0) \
                      ELSE COALESCE((SELECT SUM(t.count) FROM import_list_tab_count AS t \
                                     WHERE t.tab = v.tab AND t.item <> 'group'), 0) END \
                 - CASE WHEN v.tab = 'pending' \
                        THEN COALESCE((SELECT SUM(g.count) FROM import_list_collapsed AS c \
                                       JOIN import_list_group_entry_count AS g \
                                           ON g.watched_folder_path = c.watched_folder_path \
                                          AND g.group_name = c.group_name \
                                       WHERE c.list_id = v.id \
                                         AND (v.pending_filter = 'all' \
                                              OR g.pending_entry = v.pending_filter)), 0) \
                        ELSE 0 END \
                 + CASE WHEN v.tab <> 'pending' THEN 0 \
                        WHEN v.pending_filter = 'all' \
                        THEN COALESCE((SELECT t.count FROM import_list_tab_count AS t \
                                       WHERE t.tab = 'pending' AND t.item = 'group'), 0) \
                        ELSE (SELECT count(*) FROM import_list_group_entry_count AS g \
                              WHERE g.pending_entry = v.pending_filter) END \
             FROM import_list_view AS v WHERE v.id = ?"
        }
        (false, true) => {
            "SELECT count(*) FROM import_list_view AS v \
             JOIN import_list_item AS i ON i.list_id = v.id AND i.tab = v.tab WHERE v.id = ?"
        }
    };
    let count = sql.query_row(statement, [list.id.0], |row| row.get::<_, i64>(0))?;
    to_u64(count, "a list's item count")
}

/// How many of the tab's releases the list's filters keep, of how many it
/// has: the kept count of Found's filter entry, or, filtered by text, a count
/// over the rows the filter reads.
pub(super) fn narrowed(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    counts: &Counts,
) -> Result<NarrowedCount, DbError> {
    let shown = match list.filters_text {
        false => sql.query_row(
            "SELECT COALESCE((SELECT e.count FROM import_list_entry_count AS e \
                              WHERE e.pending_entry = v.pending_filter), 0) \
             FROM import_list_view AS v WHERE v.id = ?",
            [list.id.0],
            |row| row.get::<_, i64>(0),
        )?,
        true => sql.query_row(
            "SELECT count(*) FROM import_list_matching WHERE list_id = ? AND tab = ?",
            params![list.id.0, super::order::tab_column(list.view.tab)],
            |row| row.get::<_, i64>(0),
        )?,
    };
    Ok(NarrowedCount {
        shown: to_u32(shown, "a list's narrowed count")?,
        total: counts.tabs.of(list.view.tab),
    })
}

/// Where the first selected row sits in the list, reading the items before
/// it; `None` with nothing selected.
pub(super) fn first_selected_position(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
) -> Result<Option<u64>, DbError> {
    let selects = sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM import_list_selection WHERE list_id = ?)",
        [list.id.0],
        |row| row.get::<_, bool>(0),
    )?;
    if !selects {
        return Ok(None);
    }
    list.items(Shown::View).position_where(
        sql,
        "s.item <> 'group' AND EXISTS (SELECT 1 FROM import_list_selection AS chosen \
                                       WHERE chosen.list_id = :list AND chosen.key = s.key)",
        &[],
    )
}

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
                row.get::<_, String>(12)?,
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
        parts.push(SelectionPart {
            actions,
            count: rows,
        });
    }
    let single = match (count, single) {
        (1, Some((Item::Candidate, key))) => Some(SingleSelection::Candidate { key }),
        (1, Some((Item::Invalid, key))) => Some(SingleSelection::InvalidCandidate { key }),
        _ => None,
    };
    Ok(SelectionSummary {
        count,
        single,
        offers: selection_offers(&parts),
    })
}

/// The selected rows of the list, in its order.
pub(super) fn selected(
    sql: &(impl QueryOne + QueryRows),
    list: ImportListId,
) -> Result<SelectedRows, DbError> {
    let row = super::list_row(sql, list)?;
    let terms = order_terms(row.view.order, "p");
    let rows = sql.query(
        &format!(
            "WITH {uploading} \
             SELECT p.item, p.key, c.name, {BASIS_COLUMNS} \
             FROM import_list_selection AS s \
             JOIN import_list_placement AS p ON p.key = s.key AND p.item <> 'group' \
             JOIN scan_candidate AS c \
                 ON c.watched_folder_path = p.watched_folder_path AND c.path = p.key \
             LEFT JOIN uploading AS u ON u.key = p.key \
             WHERE s.list_id = ? \
             ORDER BY u.position IS NULL, u.position, {terms}",
            uploading = uploading_cte(),
        ),
        [list.0],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                BasisColumns::read(row, 3)?,
            ))
        },
    )?;
    let mut selected = SelectedRows::default();
    for (item, key, name, basis) in rows {
        match Item::of(&item)? {
            Item::Candidate => selected.candidates.push(SelectedCandidate {
                basis: basis.basis(&key)?,
                candidate_key: key,
                name,
            }),
            Item::Invalid | Item::Group => selected.invalid.push(key),
        }
    }
    Ok(selected)
}

/// Where the candidate at `key` sits in `list` once revealed.
pub(super) fn locate(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    key: &str,
) -> Result<Option<ImportCandidateListLocation>, DbError> {
    let row = super::list_row(sql, list)?;
    let Some((root, tab, group_name)) = sql
        .query_row(
            "SELECT watched_folder_path, tab, group_name FROM import_list_placement \
             WHERE key = ? AND item = 'candidate'",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    let tab = tab_of(&tab)?;
    let group = group_name.map(|name| crate::import::FolderReleaseDecisionKey {
        watched_folder_path: root,
        relative_folder_path: name,
    });
    let position = Items::new(
        list,
        tab,
        row.view.order,
        Shown::Revealing {
            opened: group.as_ref(),
        },
    )
    .position_of(sql, key)?;
    Ok(position.map(|position| ImportCandidateListLocation {
        stable_key: crate::import::list::candidate_stable_key(key),
        tab,
        group_key: group,
        visible_position: position,
    }))
}

/// The first candidate being identified in `list`'s order, every tab's rows
/// unfiltered, Found's first.
pub(super) fn first_identifying(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<Option<String>, DbError> {
    let row = super::list_row(sql, list)?;
    let terms = order_terms(row.view.order, "p");
    Ok(sql
        .query_row(
            &format!(
                "WITH {uploading} \
                 SELECT p.key FROM import_candidate_standing AS st \
                 JOIN import_list_placement AS p \
                     ON p.key = st.candidate_key AND p.item = 'candidate' \
                 LEFT JOIN uploading AS u ON u.key = p.key \
                 WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick') \
                 ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END, \
                          u.position IS NULL, u.position, {terms} \
                 LIMIT 1",
                uploading = uploading_cte(),
            ),
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}
