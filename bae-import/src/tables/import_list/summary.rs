//! The reads a surface makes beside a list's windows: the watched folders,
//! whether Found has groups, and what the view narrows by; the tab counts and
//! Found's filter entries, which COUNT queries over the placement return; and
//! how many items the list holds and how many its filters keep (a search
//! counts the rows its index finds). The module also holds the reads a
//! surface makes of a list on its own: where the list would place a
//! candidate once the app reveals it, and which candidate bae identifies
//! first.

use super::order::{sorted_terms, unit_join, uploading_cte};
use super::views::tab_of;
use super::*;
use crate::list::{
    ImportCandidateListLocation, ImportListCount, ImportListNarrowing, ImportQueueSummary,
    NarrowedCount,
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
pub(super) const FILTER_ENTRIES: &str = "WITH entries (position, entry) AS ( \
         VALUES (1, 'needs_you'), (2, 'in_progress'), (3, 'identified'), (4, 'unmatched'), \
                (5, 'not_looked_up')), \
     held (entry, count) AS ( \
         SELECT pending_entry, COUNT(*) FROM import_list_placement \
         WHERE pending_entry IS NOT NULL GROUP BY pending_entry), \
     counted (position, entry, count) AS ( \
         SELECT 0, 'all', COALESCE(SUM(count), 0) FROM held \
         UNION ALL \
         SELECT e.position, e.entry, COALESCE(h.count, 0) FROM entries AS e \
         LEFT JOIN held AS h ON h.entry = e.entry) \
     SELECT entry, count, count > 0 FROM counted ORDER BY position";

/// How many of Found's releases each filter entry holds, in the menu's
/// order.
pub(super) fn filter_entries(sql: &SqlReadContext<'_>) -> Result<Vec<PendingFilterEntry>, DbError> {
    sql.query(FILTER_ENTRIES, [], |row| {
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

/// The summary `list` shows beside its rows: the watched folders, whether
/// Found has any group, and what the view narrows its rows by.
pub(super) fn summary(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<ImportQueueSummary, DbError> {
    let list = list_row(sql, list)?.ok_or_else(|| closed(list))?;
    Ok(ImportQueueSummary {
        watched_folders: super::watched_folders::watched_folders_on(sql)?,
        has_groups: has_groups_on(sql)?,
        narrowing: ImportListNarrowing {
            tab: list.view.tab,
            filter_text: list.view.filter_text.clone(),
            pending_filter: list.view.pending_filter,
        },
    })
}

/// How many items `list` holds and how many of its tab's entries its
/// narrowing keeps.
pub(super) fn count(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<ImportListCount, DbError> {
    let list = list_row(sql, list)?.ok_or_else(|| closed(list))?;
    let tabs = tab_counts_on(sql)?;
    let listed = listed(sql, &list, &tabs)?;
    Ok(ImportListCount { rows: listed.total, narrowed: listed.narrowed })
}

/// The error a read of a list that is no longer open fails with: a surface
/// reads only the lists it holds open, and closing one ends its reads first.
pub(super) fn closed(list: ImportListId) -> DbError {
    DbError::unreadable("an import list read", format!("{list:?}, which is closed"))
}

/// How many items a list holds, and how many of its tab's releases its
/// filters keep of how many the tab has, while they narrow it.
pub(super) struct Listed {
    pub(super) total: u64,
    pub(super) narrowed: Option<NarrowedCount>,
}

/// What [`SEARCH_COUNTS`] counts for a searching list: how many releases its
/// filters keep, and how many items it holds.
struct SearchCounts {
    kept: i64,
    items: i64,
}

/// How many releases a searching list's filters keep, and how many items it
/// holds, both counted over the rows its search finds, which are read once:
/// the kept releases outside the groups it folds shut, and the headers among
/// its items of the groups holding a kept release.
pub(super) const SEARCH_COUNTS: &str = "WITH kept AS MATERIALIZED ( \
         SELECT m.watched_folder_path, m.group_folder, m.folded FROM import_list_found AS f \
         CROSS JOIN import_list_matching AS m ON m.list_id = f.list_id AND m.entry = f.entry \
         WHERE f.list_id = :list) \
     SELECT (SELECT count(*) FROM kept), \
            (SELECT count(*) FROM kept WHERE NOT folded) \
            + (SELECT count(*) \
               FROM (SELECT DISTINCT watched_folder_path, group_folder FROM kept \
                     WHERE group_folder IS NOT NULL) AS g \
               CROSS JOIN import_list_item AS h \
                   ON h.list_id = :list AND h.watched_folder_path = g.watched_folder_path \
                  AND h.item = 'group' AND h.key = g.group_folder)";

/// How many items a list that folds a group or narrows its tab holds: the
/// releases its filter entry keeps, less those under the groups it folds
/// shut, plus the header of each group holding a kept release.
const KEPT_ITEMS: &str = "SELECT kept - folded + headers FROM ( \
         SELECT \
             CASE WHEN v.tab = 'pending' AND v.pending_filter <> 'all' \
                  THEN (SELECT COUNT(*) FROM import_list_placement AS p \
                        WHERE p.pending_entry = v.pending_filter) \
                  ELSE (SELECT COUNT(*) FROM import_list_placement AS p \
                        WHERE p.tab = v.tab AND p.item <> 'group') END AS kept, \
             CASE WHEN v.tab = 'pending' \
                  THEN (SELECT COUNT(*) FROM import_list_collapsed AS c \
                        JOIN import_list_placement AS p \
                            ON p.watched_folder_path = c.watched_folder_path \
                           AND p.group_folder = c.group_folder AND p.item <> 'group' \
                        WHERE c.list_id = v.id \
                          AND (v.pending_filter = 'all' OR p.pending_entry = v.pending_filter)) \
                  ELSE 0 END AS folded, \
             CASE WHEN v.tab <> 'pending' THEN 0 \
                  WHEN v.pending_filter = 'all' \
                  THEN (SELECT COUNT(*) FROM import_list_placement AS h \
                        WHERE h.tab = 'pending' AND h.item = 'group') \
                  ELSE (SELECT COUNT(*) FROM import_list_placement AS h \
                        WHERE h.tab = 'pending' AND h.item = 'group' \
                          AND EXISTS (SELECT 1 FROM import_list_placement AS m \
                                      WHERE m.watched_folder_path = h.watched_folder_path \
                                        AND m.group_folder = h.key AND m.item <> 'group' \
                                        AND m.pending_entry = v.pending_filter)) END AS headers \
         FROM import_list_view AS v WHERE v.id = ?)";

/// How many items the list holds — its releases its filters keep outside the
/// groups it folds shut, and the header of each group holding one — and how
/// many releases its filters keep. Without a search these are the kept
/// counts; a search counts the rows it finds.
pub(super) fn listed(
    sql: &SqlReadContext<'_>,
    list: &ListRow,
    tabs: &TriageTabCounts,
) -> Result<Listed, DbError> {
    let narrowed = |shown: i64| -> Result<NarrowedCount, DbError> {
        Ok(NarrowedCount {
            shown: to_u32(shown, "a list's narrowed count")?,
            total: tabs.of(list.view.tab),
        })
    };
    let total = |count: i64| to_u64(count, "a list's item count");
    if list.filters_text {
        let found = sql.query_row(SEARCH_COUNTS, named_params! { ":list": list.id.0 }, |row| {
            Ok(SearchCounts { kept: row.get(0)?, items: row.get(1)? })
        })?;
        return Ok(Listed { total: total(found.items)?, narrowed: Some(narrowed(found.kept)?) });
    }
    let items = match list.keeps_all {
        true => sql.query_row(
            "SELECT COUNT(*) FROM import_list_view AS v \
             JOIN import_list_placement AS p ON p.tab = v.tab WHERE v.id = ?",
            [list.id.0],
            |row| row.get::<_, i64>(0),
        )?,
        false => sql.query_row(KEPT_ITEMS, [list.id.0], |row| row.get::<_, i64>(0))?,
    };
    let narrowed = match list.narrows {
        true => Some(narrowed(sql.query_row(
            "SELECT COUNT(*) FROM import_list_view AS v \
             JOIN import_list_placement AS p ON p.pending_entry = v.pending_filter \
             WHERE v.id = ?",
            [list.id.0],
            |row| row.get::<_, i64>(0),
        )?)?),
        false => None,
    };
    Ok(Listed { total: total(items)?, narrowed })
}

/// Where the candidate at `key` sits in `list` once revealed: `None` for a
/// candidate the list lists nowhere, and `None` around it all once the list
/// is closed.
pub(super) fn locate(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
    key: &CandidateKey,
) -> Result<Option<Option<ImportCandidateListLocation>>, DbError> {
    let Some(row) = super::list_row(sql, list)? else {
        return Ok(None);
    };
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
        return Ok(Some(None));
    };
    let tab = tab_of(&tab)?;
    let group = group_folder.map(|folder| crate::FolderReleaseDecisionKey {
        watched_folder_path: root,
        relative_folder: folder,
    });
    let position =
        Items::new(list, tab, row.view.order, Shown::Revealing { opened: group.as_ref() })
            .position_of(sql, key)?;
    Ok(Some(position.map(|(position, order_key)| ImportCandidateListLocation {
        stable_key: crate::list::candidate_stable_key(key),
        tab,
        group_key: group,
        visible_position: position,
        key: order_key,
    })))
}

/// The first candidate being identified in `list`'s order, every tab's rows
/// unfiltered, Found's first: `None` for none, and `None` around it all once
/// the list is closed.
pub(super) fn first_identifying(
    sql: &SqlReadContext<'_>,
    list: ImportListId,
) -> Result<Option<Option<CandidateKey>>, DbError> {
    let Some(row) = super::list_row(sql, list)? else {
        return Ok(None);
    };
    Ok(Some(
        sql.query_row(
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
                terms = sorted_terms(row.view.order, "p", "unit"),
            ),
            [],
            |row| row.get::<_, CandidateKey>(0),
        )
        .optional()?,
    ))
}

impl ImportTables {
    /// How many entries each tab holds now.
    pub async fn load_tab_counts(&self) -> Result<TriageTabCounts, DbError> {
        self.store.read(move |sql| tab_counts_on(&sql)).await
    }

    /// How many entries each tab holds, live: a value for every write that
    /// moves one. Read apart from the list, and sent on its own, because a
    /// scan moves them at every commit and a list narrowed to one tab has no
    /// other use for the rest.
    pub fn subscribe_tab_counts(&self) -> coven::LiveQuery<TriageTabCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| tab_counts_on(&sql))
    }

    /// Where the candidate at `candidate_key` sits in the list `list` once
    /// revealed: on the tab its placement puts it on, with the filters
    /// cleared and its group opened, the list's other folds kept. `None` when
    /// the list lists no such candidate.
    pub async fn locate_import_candidate(
        &self,
        list: ImportListId,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<crate::ImportCandidateListLocation>, ImportListError> {
        let candidate_key = candidate_key.clone();
        self.store
            .read(move |sql| locate(&sql, list, &candidate_key))
            .await?
            .ok_or(ImportListError::Closed(list))
    }

    /// The first candidate in the list's order, every tab's rows unfiltered,
    /// whose identification is queued, running or having its answer written;
    /// `None` when none is.
    pub(crate) async fn first_identifying_import_candidate(
        &self,
        list: ImportListId,
    ) -> Result<Option<crate::CandidateKey>, ImportListError> {
        self.store
            .read(move |sql| first_identifying(&sql, list))
            .await?
            .ok_or(ImportListError::Closed(list))
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
