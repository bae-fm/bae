//! The reads a surface makes beside a list's windows: whether bae watches
//! any folder and whether Found has groups; the tab counts and Found's filter
//! entries, which the counts kept beside the placement (`import_list_count`)
//! hold; and how many items the list holds and how many its filters keep (a
//! search counts the rows its index finds). The module also holds the reads
//! a surface makes of a list on its own: the view that reveals a candidate,
//! where the list shows it, and which candidate bae identifies first.

use super::order::{sorted_terms, unit_join, uploading_cte};
use super::*;
use crate::list::{ImportListCount, ImportListReveal, ImportQueueSummary, NarrowedCount};
use crate::RelativeFolder;
use crate::{CandidateKey, PendingFilterEntry, TriageTabCounts};

/// Every tab's count: the releases and folders that failed validation it
/// lists, which is every placement row but a group's header.
pub(super) const TAB_COUNTS: &str =
    "SELECT COALESCE(SUM(count) FILTER (WHERE tab = 'pending'), 0), \
            COALESCE(SUM(count) FILTER (WHERE tab = 'done'), 0), \
            COALESCE(SUM(count) FILTER (WHERE tab = 'skipped'), 0) \
     FROM import_list_count WHERE item <> 'group'";

/// The statement summing the counts kept beside the placement that `test`,
/// over the count row aliased `c`, keeps: how many placement rows those
/// counts hold.
fn counted(test: &str) -> String {
    format!("SELECT COALESCE(SUM(c.count), 0) FROM import_list_count AS c WHERE {test}")
}

/// Every tab's count: what the tab bar shows, whatever a list is narrowed by.
pub(super) fn tab_counts_on(sql: &SqlReadContext<'_>) -> Result<TriageTabCounts, DbError> {
    tab_counts(sql.query_row(TAB_COUNTS, [], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
    })?)
}

/// The tab counts [`TAB_COUNTS`] selects, in its columns' order.
fn tab_counts((pending, done, skipped): (i64, i64, i64)) -> Result<TriageTabCounts, DbError> {
    Ok(TriageTabCounts {
        pending: to_u32(pending, "the pending tab's count")?,
        done: to_u32(done, "the done tab's count")?,
        skipped: to_u32(skipped, "the skipped tab's count")?,
    })
}

/// Whether Found has any group: a group's header is a placement row of its
/// own, which the counts count.
const HAS_GROUPS: &str =
    "SELECT EXISTS (SELECT 1 FROM import_list_count WHERE tab = 'pending' AND item = 'group')";

fn has_groups_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(sql.query_row(HAS_GROUPS, [], |row| row.get(0))?)
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
             SELECT pending_entry, count FROM import_list_count \
             WHERE pending_entry IS NOT NULL), \
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
/// entries its narrowing keeps, and every tab's count, in one statement: the
/// tab counts beside the list's own counts, so a narrowed list's total is the
/// tab bar's count of its tab.
pub(super) fn count(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<ImportListCount, DbError> {
    let items = Items::new(request.clone());
    let (tabs, rows, shown) = items
        .query(sql, &count_statement(request), &[], |row| {
            Ok((
                (row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?),
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?
        .into_iter()
        .next()
        .expect("a count answers one row");
    let tabs = tab_counts(tabs)?;
    let narrowed = match shown {
        Some(shown) => Some(NarrowedCount {
            shown: to_u32(shown, "a list's narrowed count")?,
            total: tabs.of(request.view.tab),
        }),
        None => None,
    };
    Ok(ImportListCount { rows: to_u64(rows, "a list's item count")?, narrowed, tabs })
}

/// The statement [`count`] reads: every tab's count, then how many items the
/// list `request` shows holds and how many of its tab's entries its
/// narrowing keeps.
fn count_statement(request: &ImportListRequest) -> String {
    format!(
        "SELECT tabs.*, listed.items, listed.shown FROM ({TAB_COUNTS}) AS tabs, ({listed}) AS listed",
        listed = listed(request),
    )
}

/// The statement counting how many releases a searching list's filters keep
/// (`shown`), and how many items it holds (`items`), both over the rows its
/// search finds, which are read once: the kept releases outside the groups it
/// folds shut, and the headers among its items of the groups holding a kept
/// release.
pub(super) fn search_counts(request: &ImportListRequest) -> String {
    format!(
        "WITH kept AS MATERIALIZED ( \
             SELECT m.watched_folder_path, m.group_folder, m.folded FROM {found} AS f \
             CROSS JOIN {matching} AS m ON m.entry = f.entry) \
         SELECT (SELECT count(*) FROM kept) AS shown, \
                (SELECT count(*) FROM kept WHERE NOT folded) \
                + (SELECT count(*) \
                   FROM (SELECT DISTINCT watched_folder_path, group_folder FROM kept \
                         WHERE group_folder IS NOT NULL) AS g \
                   CROSS JOIN {items} AS h \
                       ON h.watched_folder_path = g.watched_folder_path \
                      AND h.item = 'group' AND h.key = g.group_folder) AS items",
        found = request.found(),
        matching = request.matching(),
        items = request.items(),
    )
}

/// The statement counting how many items a list that searches for nothing
/// holds, from the counts kept beside the placement alone: the releases its
/// filter entry keeps, less those under the groups it folds shut, plus the
/// header of each group holding a kept release. The group counts
/// (`import_list_group_count`) hold each group's members in each entry, so
/// they give the folded members and, under an entry, the groups holding one
/// of its releases; with no entry, every group's header counts.
fn kept_items(request: &ImportListRequest) -> String {
    let pending = request.view.tab == TriageTab::Pending;
    let kept = match request.entry_sql("c") {
        Some(entry) => counted(&entry),
        None => counted(&format!(
            "c.tab = {tab} AND c.item <> 'group'",
            tab = tab_literal(request.view.tab)
        )),
    };
    let folded = match pending {
        true => format!(
            "SELECT COALESCE(SUM(g.count), 0) FROM import_list_group_count AS g \
             WHERE {folds} AND {entry}",
            folds = request.folds_sql("g.watched_folder_path", "g.group_folder"),
            entry = request.entry_sql("g").unwrap_or_else(|| "1".to_string()),
        ),
        false => "SELECT 0".to_string(),
    };
    let headers = match (pending, request.entry_sql("g")) {
        (false, _) => "SELECT 0".to_string(),
        (true, None) => counted("c.tab = 'pending' AND c.item = 'group'"),
        (true, Some(entry)) => {
            format!("SELECT COUNT(*) FROM import_list_group_count AS g WHERE {entry}")
        }
    };
    format!("SELECT ({kept}) - ({folded}) + ({headers})")
}

/// The statement counting how many items the list shows (`items`) — its
/// releases its filters keep outside the groups it folds shut, and the header
/// of each group holding one — and how many releases its filters keep while
/// they narrow it (`shown`), NULL while they don't. Without a search these
/// are the kept counts; a search counts the rows it finds.
fn listed(request: &ImportListRequest) -> String {
    if request.filters_text() {
        return search_counts(request);
    }
    let items = kept_items(request);
    // Without a search, only Found's filter entry narrows the list.
    let shown = match request.entry_sql("c") {
        Some(entry) => counted(&entry),
        None => "SELECT NULL".to_string(),
    };
    format!("SELECT ({items}) AS items, ({shown}) AS shown")
}

/// The CTE a reveal starts from: the candidate's placement row, by its key.
const REVEALED: &str = "revealed AS ( \
     SELECT * FROM import_list_placement WHERE key = :key AND item = 'candidate')";

/// The group a reveal opens, whatever the list's folds say: the candidate's.
const REVEALED_GROUP: &str = "SELECT watched_folder_path, group_folder FROM revealed \
     WHERE group_folder IS NOT NULL";

/// The request that reveals a candidate on `tab` from the list `request`
/// shows: the filters cleared, the order and the folds kept.
fn revealing(request: &ImportListRequest, tab: TriageTab) -> ImportListRequest {
    ImportListRequest {
        view: crate::ImportListView {
            tab,
            search: None,
            pending_filter: PendingFilter::All,
            order: request.view.order,
        },
        folds: request.folds.clone(),
    }
}

/// The statement revealing the candidate at `:key` from the list `request`
/// shows, in one read: it finds the candidate's placement row by its key,
/// and an arm for each part of each tab counts, when the row is on that tab
/// and part, the items before it in the tab's order with the filters cleared
/// and the candidate's group opened, off the order's index. It selects the
/// tab, the group and the position.
pub(super) fn reveal_statement(request: &ImportListRequest) -> String {
    let (ctes, arms): (Vec<String>, Vec<String>) =
        [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped]
            .into_iter()
            .flat_map(|tab| Items::revealing(revealing(request, tab), REVEALED_GROUP).reveal_arms())
            .unzip();
    format!(
        "WITH {REVEALED}, {uploading}, {ctes} {arms}",
        uploading = uploading_cte().replacen(" AS (", " AS MATERIALIZED (", 1),
        ctes = ctes.join(", "),
        arms = arms.join(" UNION ALL ")
    )
}

/// What revealing the candidate at `key` from the list `request` shows: the
/// request on the tab its placement puts it on, the filters cleared, its
/// group opened and the list's order and other folds kept, and where it sits
/// there. `None` for a candidate no tab lists.
pub(super) fn reveal(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
    key: &CandidateKey,
) -> Result<Option<ImportListReveal>, DbError> {
    // Every tab's arm binds the same values: no search, no filter entry, and
    // the request's folds.
    let items = Items::revealing(revealing(request, TriageTab::Pending), REVEALED_GROUP);
    let Some((tab, root, group_folder, position)) = items
        .query(sql, &reveal_statement(request), &[(":key", key)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                path_column::get(row, 1)?,
                row.get::<_, Option<RelativeFolder>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let mut revealed = revealing(request, tab_of(&tab)?);
    if let Some(folder) = group_folder {
        let group =
            crate::FolderReleaseDecisionKey { watched_folder_path: root, relative_folder: folder };
        match &mut revealed.folds {
            ImportListFolds::Only(folded) => {
                folded.remove(&group);
            }
            ImportListFolds::AllBut(open) => {
                open.insert(group);
            }
        }
    }
    Ok(Some(ImportListReveal {
        request: revealed,
        position: to_u64(position, "a revealed candidate's position")?,
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

    /// What revealing the candidate at `candidate_key` from the list
    /// `request` shows, and where the candidate sits there; `None` when no
    /// tab lists such a candidate.
    pub async fn reveal_import_candidate(
        &self,
        request: ImportListRequest,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<ImportListReveal>, DbError> {
        let candidate_key = candidate_key.clone();
        self.store.read(move |sql| reveal(&sql, &request, &candidate_key)).await
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
