//! The reads a surface makes beside a list's windows: whether bae watches
//! any folder and whether Found has groups; the tab counts and Found's filter
//! entries, which COUNT queries over the placement return; and how many items
//! the list holds and how many its filters keep (a search counts the rows its
//! index finds). The module also holds the reads a surface makes of a list on
//! its own: the view that reveals a candidate, where the list shows it, and
//! which candidate bae identifies first.

use super::order::{sorted_terms, unit_join, uploading_cte};
use super::*;
use crate::list::{
    ImportCandidateListLocation, ImportListCount, ImportQueueSummary, NarrowedCount,
};
use crate::RelativeFolder;
use crate::{CandidateKey, PendingFilterEntry, TriageTabCounts};

/// Every tab's count: the releases and folders that failed validation it
/// lists, which is every placement row but a group's header.
pub(super) const TAB_COUNTS: &str = "SELECT COUNT(*) FILTER (WHERE tab = 'pending'), \
                                 COUNT(*) FILTER (WHERE tab = 'done'), \
                                 COUNT(*) FILTER (WHERE tab = 'skipped') \
                          FROM import_list_placement WHERE item <> 'group'";

/// Every tab's count: what the tab bar shows, whatever a list is narrowed by.
pub(super) fn tab_counts_on(sql: &SqlReadContext<'_>) -> Result<TriageTabCounts, DbError> {
    let (pending, done, skipped) = sql.query_row(TAB_COUNTS, [], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
    })?;
    Ok(TriageTabCounts {
        pending: to_u32(pending, "the pending tab's count")?,
        done: to_u32(done, "the done tab's count")?,
        skipped: to_u32(skipped, "the skipped tab's count")?,
    })
}

/// Whether Found has any group: a group's header is a placement row of its
/// own.
fn has_groups_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(sql.query_row(
        "SELECT EXISTS (SELECT 1 FROM import_list_placement WHERE tab = 'pending' AND item = 'group')",
        [],
        |row| row.get(0),
    )?)
}

/// Every entry of Found's filter, in the menu's order, with how many of
/// Found's releases it holds: `all` holds every one and the others each hold
/// those in their states. The person cannot choose an entry that holds no
/// releases.
pub(super) fn filter_entries_sql() -> String {
    let entries = PendingFilter::ENTRIES
        .iter()
        .enumerate()
        .filter(|(_, filter)| **filter != PendingFilter::All)
        .map(|(position, filter)| format!("({position}, '{}')", pending_filter_column(*filter)))
        .collect::<Vec<_>>()
        .join(", ");
    let all = pending_filter_column(PendingFilter::All);
    format!(
        "WITH entries (position, entry) AS (VALUES {entries}), \
         held (entry, count) AS ( \
             SELECT pending_entry, COUNT(*) FROM import_list_placement \
             WHERE pending_entry IS NOT NULL GROUP BY pending_entry), \
         counted (position, entry, count) AS ( \
             SELECT 0, '{all}', COALESCE(SUM(count), 0) FROM held \
             UNION ALL \
             SELECT e.position, e.entry, COALESCE(h.count, 0) FROM entries AS e \
             LEFT JOIN held AS h ON h.entry = e.entry) \
         SELECT entry, count, count > 0 FROM counted ORDER BY position"
    )
}

/// How many of Found's releases each filter entry holds, in the menu's
/// order.
pub(super) fn filter_entries(sql: &SqlReadContext<'_>) -> Result<Vec<PendingFilterEntry>, DbError> {
    sql.query(&filter_entries_sql(), [], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, bool>(2)?))
    })?
    .into_iter()
    .map(|(entry, count, selectable)| {
        Ok(PendingFilterEntry {
            filter: pending_filter_of(&entry)?,
            count: to_u32(count, "a filter entry's count")?,
            selectable,
        })
    })
    .collect()
}

/// The summary a surface shows beside the list's rows: whether bae watches
/// any folder, and whether Found has any group.
pub(super) fn summary(sql: &SqlReadContext<'_>) -> Result<ImportQueueSummary, DbError> {
    Ok(ImportQueueSummary {
        has_watched_folders: super::watched_folders::any_watched_on(sql)?,
        has_groups: has_groups_on(sql)?,
    })
}

/// How many items the list `request` shows holds, how many of its tab's
/// entries its narrowing keeps, and every tab's count.
pub(super) fn count(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<ImportListCount, DbError> {
    let tabs = tab_counts_on(sql)?;
    let listed = listed(sql, request)?;
    Ok(ImportListCount { rows: listed.total, narrowed: listed.narrowed, tabs })
}

/// How many items a list holds, and how many of its tab's releases its
/// filters keep of how many the tab has, while they narrow it.
pub(super) struct Listed {
    pub(super) total: u64,
    pub(super) narrowed: Option<NarrowedCount>,
}

/// What [`search_counts`] counts for a searching list: how many releases its
/// filters keep, and how many items it holds.
struct SearchCounts {
    kept: i64,
    items: i64,
}

/// The statement counting how many releases a searching list's filters keep,
/// and how many items it holds, both over the rows its search finds, which
/// are read once: the kept releases outside the groups it folds shut, and the
/// headers among its items of the groups holding a kept release.
pub(super) fn search_counts(request: &ImportListRequest) -> String {
    format!(
        "WITH kept AS MATERIALIZED ( \
             SELECT m.watched_folder_path, m.group_folder, m.folded FROM {found} AS f \
             CROSS JOIN {matching} AS m ON m.entry = f.entry) \
         SELECT (SELECT count(*) FROM kept), \
                (SELECT count(*) FROM kept WHERE NOT folded) \
                + (SELECT count(*) \
                   FROM (SELECT DISTINCT watched_folder_path, group_folder FROM kept \
                         WHERE group_folder IS NOT NULL) AS g \
                   CROSS JOIN {items} AS h \
                       ON h.watched_folder_path = g.watched_folder_path \
                      AND h.item = 'group' AND h.key = g.group_folder)",
        found = request.found(),
        matching = request.matching(),
        items = request.items(),
    )
}

/// The statement counting how many items a list that folds a group or narrows
/// its tab holds: the releases its filter entry keeps, less those under the
/// groups it folds shut, plus the header of each group holding a kept
/// release.
fn kept_items(request: &ImportListRequest) -> String {
    let entry = request.view.pending_filter != PendingFilter::All;
    let pending = request.view.tab == TriageTab::Pending;
    let kept = match pending && entry {
        true => {
            "SELECT COUNT(*) FROM import_list_placement AS p \
                 WHERE p.pending_entry = :filter"
        }
        false => {
            "SELECT COUNT(*) FROM import_list_placement AS p \
                  WHERE p.tab = :tab AND p.item <> 'group'"
        }
    };
    let folded = match pending {
        true => format!(
            "SELECT COUNT(*) FROM import_list_placement AS p \
             WHERE p.item <> 'group' AND p.group_folder IS NOT NULL \
               AND {folds} AND {entry}",
            folds = request.folds_sql("p.watched_folder_path", "p.group_folder"),
            entry = match entry {
                true => "p.pending_entry = :filter",
                false => "1",
            },
        ),
        false => "SELECT 0".to_string(),
    };
    let headers = match (pending, entry) {
        (false, _) => "SELECT 0",
        (true, false) => {
            "SELECT COUNT(*) FROM import_list_placement AS h \
             WHERE h.tab = 'pending' AND h.item = 'group'"
        }
        (true, true) => {
            "SELECT COUNT(*) FROM import_list_placement AS h \
             WHERE h.tab = 'pending' AND h.item = 'group' \
               AND EXISTS (SELECT 1 FROM import_list_placement AS m \
                           WHERE m.watched_folder_path = h.watched_folder_path \
                             AND m.group_folder = h.key AND m.item <> 'group' \
                             AND m.pending_entry = :filter)"
        }
    };
    format!("SELECT ({kept}) - ({folded}) + ({headers})")
}

/// How many items the list shows — its releases its filters keep outside the
/// groups it folds shut, and the header of each group holding one — and how
/// many releases its filters keep. Without a search these are the kept
/// counts; a search counts the rows it finds.
pub(super) fn listed(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<Listed, DbError> {
    let items = Items::new(request.clone());
    let count = |statement: &str| -> Result<i64, DbError> {
        Ok(items
            .query(sql, statement, &[], |row| row.get::<_, i64>(0))?
            .into_iter()
            .next()
            .expect("a count answers one row"))
    };
    // How many entries the tab holds, which a narrowed count is of.
    let tab_total =
        || count("SELECT COUNT(*) FROM import_list_placement WHERE tab = :tab AND item <> 'group'");
    let narrowed = |shown: i64| -> Result<NarrowedCount, DbError> {
        Ok(NarrowedCount {
            shown: to_u32(shown, "a list's narrowed count")?,
            total: to_u32(tab_total()?, "a tab's count")?,
        })
    };
    let total = |count: i64| to_u64(count, "a list's item count");
    if request.filters_text() {
        let found = items
            .query(sql, &search_counts(request), &[], |row| {
                Ok(SearchCounts { kept: row.get(0)?, items: row.get(1)? })
            })?
            .into_iter()
            .next()
            .expect("a count answers one row");
        return Ok(Listed { total: total(found.items)?, narrowed: Some(narrowed(found.kept)?) });
    }
    let held = match request.keeps_all() {
        true => count("SELECT COUNT(*) FROM import_list_placement WHERE tab = :tab")?,
        false => count(&kept_items(request))?,
    };
    let narrowed = match request.narrows() {
        true => Some(narrowed(count(
            "SELECT COUNT(*) FROM import_list_placement WHERE pending_entry = :filter",
        )?)?),
        false => None,
    };
    Ok(Listed { total: total(held)?, narrowed })
}

/// The view that reveals the candidate at `key`: its tab, with the filters
/// cleared and its group opened. `None` for a candidate no tab lists.
pub(super) fn reveal_view(
    sql: &SqlReadContext<'_>,
    key: &CandidateKey,
) -> Result<Option<ImportCandidateListLocation>, DbError> {
    let Some((root, tab, group_folder)) = sql
        .query_row(
            "SELECT watched_folder_path, tab, group_folder FROM import_list_placement \
             WHERE key = ? AND item = 'candidate'",
            [key],
            |row| {
                Ok((
                    path_column::get(row, 0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<RelativeFolder>>(2)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    Ok(Some(ImportCandidateListLocation {
        tab: tab_of(&tab)?,
        group_key: group_folder.map(|folder| crate::FolderReleaseDecisionKey {
            watched_folder_path: root,
            relative_folder: folder,
        }),
    }))
}

/// The first candidate being identified in the order `order` runs, every
/// tab's rows unfiltered, Found's first: `None` for none.
pub(super) fn first_identifying(
    sql: &SqlReadContext<'_>,
    order: crate::ImportListOrder,
) -> Result<Option<CandidateKey>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "WITH {uploading} \
                 SELECT p.key FROM import_candidate_standing AS st \
                 JOIN import_list_placement AS p \
                     ON p.key = st.candidate_key AND p.item = 'candidate' \
                 {unit} \
                 LEFT JOIN uploading AS u ON u.key = p.key \
                 WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick') \
                 ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END, \
                          u.position IS NULL, u.position, {terms} \
                 LIMIT 1",
                uploading = uploading_cte(),
                unit = unit_join("p", "unit"),
                terms = sorted_terms(order, "p", "unit"),
            ),
            [],
            |row| row.get::<_, CandidateKey>(0),
        )
        .optional()?)
}

impl ImportTables {
    /// How many entries each tab holds now.
    pub async fn load_tab_counts(&self) -> Result<TriageTabCounts, DbError> {
        self.store.read(move |sql| tab_counts_on(&sql)).await
    }

    /// How many entries each tab holds, live, for a test that waits on a
    /// scan: a screen reads them in its list's count.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_tab_counts(&self) -> coven::LiveQuery<TriageTabCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| tab_counts_on(&sql))
    }

    /// The view that reveals the candidate at `candidate_key`: the tab its
    /// placement puts it on, with the filters cleared and its group opened.
    /// `None` when no tab lists such a candidate.
    pub async fn locate_import_candidate(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<crate::ImportCandidateListLocation>, DbError> {
        let candidate_key = candidate_key.clone();
        self.store.read(move |sql| reveal_view(&sql, &candidate_key)).await
    }

    /// Where the candidate at `candidate_key` sits in what the list `request`
    /// shows; `None` when it does not show it.
    pub async fn import_candidate_position(
        &self,
        request: ImportListRequest,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<u64>, DbError> {
        let candidate_key = candidate_key.clone();
        self.store.read(move |sql| Items::new(request).position_of(&sql, &candidate_key)).await
    }

    /// The first candidate in the order `order`, every tab's rows unfiltered,
    /// whose identification is queued, running or having its answer written;
    /// `None` when none is.
    pub(crate) async fn first_identifying_import_candidate(
        &self,
        order: crate::ImportListOrder,
    ) -> Result<Option<crate::CandidateKey>, DbError> {
        self.store.read(move |sql| first_identifying(&sql, order)).await
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
