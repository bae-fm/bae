//! The reads a surface makes beside a list's windows: whether bae watches
//! any folder and whether Found has groups; the tab counts and Found's filter
//! entries, which the counts kept beside the placement (`import_list_count`)
//! hold; and how many items the list holds and how many its filters keep (a
//! search counts the rows its index finds). The module also holds the reads
//! a surface makes of a list on its own: the view that reveals a candidate,
//! where the list shows it, and which candidate bae identifies first.

use super::list_items::{order_name, ListBinding};
use super::*;
use crate::list::{ImportListCount, ImportListReveal, ImportQueueSummary, NarrowedCount};
use crate::{CandidateKey, PendingFilterEntry, TriageTabCounts};

// Every tab's count: the releases and folders that failed validation it
// lists, which is every placement row but a group's header.
coven::fragment!(
    TabCounts { pending: i64, done: i64, skipped: i64 },
    r#"
    SELECT COALESCE(SUM(count) FILTER (WHERE tab = 'pending'), 0) AS pending,
        COALESCE(SUM(count) FILTER (WHERE tab = 'done'), 0) AS done,
        COALESCE(SUM(count) FILTER (WHERE tab = 'skipped'), 0) AS skipped
    FROM import_list_count
    WHERE item <> 'group'
"#
);

impl TabCounts {
    fn counts(&self) -> Result<TriageTabCounts, DbError> {
        Ok(TriageTabCounts {
            pending: to_u32(self.pending, "the pending tab's count")?,
            done: to_u32(self.done, "the done tab's count")?,
            skipped: to_u32(self.skipped, "the skipped tab's count")?,
        })
    }
}

// Whether bae watches any folder, and whether Found has any group: a group's
// header is a placement row of its own, which the counts count.
coven::read!(
    writer Summary { has_watched_folders: bool, has_groups: bool },
    r#"
    SELECT EXISTS (
               SELECT 1 FROM @crate::tables::watched_folders::fragments::WatchedFolderRows
                   AS watched) AS has_watched_folders,
           EXISTS (
               SELECT 1 FROM import_list_count
               WHERE tab = 'pending' AND item = 'group') AS has_groups
"#
);

// Every entry of Found's filter, in the order of `entries`, a JSON array of
// them, with how many of Found's releases each holds: `all` holds every one
// and the others each hold those in their states. The person cannot choose an
// entry that holds no releases.
coven::read!(
    writer FilterEntries(entries: String, all: String) {
        entry: Option<String>, count: i64, selectable: bool
    },
    r#"
    WITH named(position, entry) AS (
        SELECT listed.key, listed.value FROM json_each(:entries) AS listed
    ), held(entry, count) AS (
        SELECT import_list_count.pending_entry, import_list_count.count FROM import_list_count
        WHERE import_list_count.pending_entry IS NOT NULL
    ), counted(position, entry, count) AS (
        SELECT named.position, named.entry,
            CASE WHEN named.entry = :all
                THEN (SELECT COALESCE(SUM(every.count), 0) FROM held AS every)
                ELSE COALESCE(held.count, 0) END
        FROM named
        LEFT JOIN held ON held.entry = named.entry
    )
    SELECT counted.entry, counted.count, counted.count > 0 AS selectable
    FROM counted
    ORDER BY counted.position
"#
);

// How many items a list that searches for nothing holds, and, while Found's
// filter entry narrows it, how many releases that entry keeps, from the
// counts kept beside the placement alone: the releases the entry keeps, less
// those under the groups the list folds shut, plus the header of each group
// holding a kept release. The group counts hold each group's members in each
// entry, so they give the folded members and, under an entry, the groups
// holding one of its releases; with no entry, every group's header counts.
// `entries` is a JSON array of the entries the list keeps, every one of them
// while no entry narrows it, which the group counts are searched by. Only
// Found has groups.
coven::read!(
    writer KeptCount(
        tab: String, filter: Option<String>, entries: String, folds_only: bool, folded: String
    ) { rows: i64, shown: Option<i64> },
    r#"
    WITH releases(kept) AS MATERIALIZED (
        SELECT COALESCE(SUM(c.count), 0) FROM import_list_count AS c
        WHERE c.tab = :tab AND c.item <> 'group'
          AND (:filter IS NULL OR c.pending_entry = :filter)
    )
    SELECT tabs.*,
        releases.kept
        - (SELECT COALESCE(SUM(g.count), 0) FROM import_list_group_count AS g
           WHERE :tab = 'pending'
             AND g.pending_entry IN (SELECT kept.value FROM json_each(:entries) AS kept)
             AND ((g.watched_folder_path, g.group_folder) IN (
                      SELECT named.named_root, named.named_folder
                      FROM @crate::tables::import_list::row_fragments::NamedGroups(folded: :folded)
                          AS named)) = :folds_only)
        + (SELECT COALESCE(SUM(c.count), 0) FROM import_list_count AS c
           WHERE :filter IS NULL AND :tab = 'pending' AND c.tab = 'pending'
             AND c.item = 'group')
        + (SELECT count(*) FROM import_list_group_count AS g
           WHERE :tab = 'pending' AND g.pending_entry = :filter) AS rows,
        iif(:filter IS NULL, NULL, releases.kept) AS shown
    FROM releases
    CROSS JOIN @crate::tables::import_list::summary::TabCounts AS tabs
"#
);

// The entries of the search index whose shown text matches the pattern of
// `filter_text`, every tab's, as the index finds them: what a search's counts
// read instead of testing every row of the tab.
coven::fragment!(
    SearchFound(filter_text: String) { found_entry: i64 },
    r#"
    SELECT s.rowid AS found_entry
    FROM import_list_search AS s
    WHERE s.title GLOB bae_search_pattern(:filter_text)
       OR s.artists GLOB bae_search_pattern(:filter_text)
"#
);

// How many releases a searching list's filters keep, folded groups' members
// included (`shown`), and how many items it holds (`rows`), both over the
// rows its search finds, which are read once: the kept releases outside the
// groups it folds shut, and the headers of the groups holding a kept release.
coven::read!(
    writer SearchCount(
        tab: String, filter: Option<String>, filter_text: String, folds_only: bool,
        folded: String
    ) { rows: i64, shown: i64 },
    r#"
    WITH kept AS MATERIALIZED (
        SELECT m.watched_folder_path, m.group_folder,
            COALESCE(@crate::tables::import_list::row_fragments::Folded(
                folds_only: :folds_only, folded: :folded, parent: m), 0) AS folded
        FROM @crate::tables::import_list::summary::SearchFound(filter_text: :filter_text) AS f
        CROSS JOIN import_list_placement AS m ON m.entry = f.found_entry
        WHERE +m.tab = :tab AND m.item <> 'group'
          AND (:filter IS NULL OR m.pending_entry = :filter)
    )
    SELECT tabs.*,
        (SELECT count(*) FROM kept WHERE NOT kept.folded)
        + (SELECT count(*)
           FROM (SELECT DISTINCT kept.watched_folder_path, kept.group_folder FROM kept
                 WHERE kept.group_folder IS NOT NULL) AS g
           CROSS JOIN import_list_placement AS h
               ON h.watched_folder_path = g.watched_folder_path
              AND h.item = 'group' AND h.key = g.group_folder) AS rows,
        (SELECT count(*) FROM kept) AS shown
    FROM @crate::tables::import_list::summary::TabCounts AS tabs
"#
);

/// Every entry of Found's filter, in the menu's order, with how many of
/// Found's releases it holds.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn filter_entries(
    sql: &impl bae_store::WriterRead,
) -> Result<Vec<PendingFilterEntry>, DbError> {
    filter_entries_of(statement_rows(sql, &filter_entries_params())?)
}

/// Found's filter entries as their read returns them.
pub(super) fn filter_entries_of(
    rows: Vec<FilterEntriesRow>,
) -> Result<Vec<PendingFilterEntry>, DbError> {
    rows.into_iter()
        .map(|row| {
            let entry = row
                .entry
                .ok_or_else(|| DbError::unreadable("an entry of Found's filter", None::<String>))?;
            Ok(PendingFilterEntry {
                filter: pending_filter_of(&entry)?,
                count: to_u32(row.count, "a filter entry's count")?,
                selectable: row.selectable,
            })
        })
        .collect()
}

pub(super) fn filter_entries_params() -> FilterEntriesParams {
    FilterEntriesParams {
        entries: entry_list(&PendingFilter::ENTRIES),
        all: pending_filter_column(PendingFilter::All).to_string(),
    }
}

/// `filters` as the JSON array of entries a statement binds.
fn entry_list(filters: &[PendingFilter]) -> String {
    let entries: Vec<&str> = filters.iter().copied().map(pending_filter_column).collect();
    serde_json::to_string(&entries).expect("a list of text serializes")
}

/// The summary a surface shows beside the list's rows: whether bae watches
/// any folder, and whether Found has any group.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn summary(sql: &impl bae_store::WriterRead) -> Result<ImportQueueSummary, DbError> {
    Ok(summary_of(statement_row(sql, &SummaryParams {})?))
}

/// The summary as its row holds it.
pub(super) fn summary_of(row: SummaryRow) -> ImportQueueSummary {
    ImportQueueSummary { has_watched_folders: row.has_watched_folders, has_groups: row.has_groups }
}

/// The statement counting a list: the read of its search's rows while it
/// searches, else of the kept counts, with what it binds.
pub(super) enum CountStatement {
    Kept(KeptCountParams),
    Search(SearchCountParams),
}

impl CountStatement {
    pub(super) fn of(request: &ImportListRequest) -> Self {
        let ListBinding { tab, filter, filter_text, folds_only, folded } = ListBinding::of(request);
        match filter_text {
            Some(filter_text) => {
                Self::Search(SearchCountParams { tab, filter, filter_text, folds_only, folded })
            }
            None => {
                let entries = match request.entry() {
                    Some(entry) => entry_list(&[entry]),
                    None => entry_list(&PendingFilter::ENTRIES),
                };
                Self::Kept(KeptCountParams { tab, filter, entries, folds_only, folded })
            }
        }
    }
}

/// How many items the list `request` shows holds, how many of its tab's
/// entries its narrowing keeps, and every tab's count, in one statement: the
/// tab counts beside the list's own counts, so a narrowed list's total is the
/// tab bar's count of its tab.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn count(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
) -> Result<ImportListCount, DbError> {
    match CountStatement::of(request) {
        CountStatement::Kept(params) => {
            let row = statement_row(sql, &params)?;
            count_of(request, row.tabs.counts()?, row.rows, row.shown)
        }
        CountStatement::Search(params) => {
            let row = statement_row(sql, &params)?;
            count_of(request, row.tabs.counts()?, row.rows, Some(row.shown))
        }
    }
}

/// How many items the list `request` shows holds and how many its narrowing
/// keeps, live.
pub(super) fn subscribe_count(
    store: &Store,
    request: ImportListRequest,
) -> coven::LiveQuery<ImportListCount, DbError> {
    match CountStatement::of(&request) {
        CountStatement::Kept(params) => store
            .subscribe_row(params)
            .process(move |row| count_of(&request, row.tabs.counts()?, row.rows, row.shown)),
        CountStatement::Search(params) => store
            .subscribe_row(params)
            .process(move |row| count_of(&request, row.tabs.counts()?, row.rows, Some(row.shown))),
    }
}

/// The count of the list `request` shows from its count statement's row: the
/// counts of the tabs, the list's rows and, while it narrows, the rows it
/// shows.
fn count_of(
    request: &ImportListRequest,
    tabs: TriageTabCounts,
    rows: i64,
    shown: Option<i64>,
) -> Result<ImportListCount, DbError> {
    let narrowed = match shown {
        Some(shown) => Some(NarrowedCount {
            shown: to_u32(shown, "a list's narrowed count")?,
            total: tabs.of(request.view.tab),
        }),
        None => None,
    };
    Ok(ImportListCount { rows: to_u64(rows, "a list's item count")?, narrowed, tabs })
}

// Where the candidate at `opened` shows once revealed: its tab, its group,
// and how many items sit before it among its tab's, in the list's order with
// the filters cleared and its group opened.
coven::read!(
    writer Revealed(
        list_order: String, tab: Option<String>, filter: Option<String>,
        filter_text: Option<String>, folds_only: bool, folded: String,
        opened: Option<crate::CandidateKey>
    ) { group_folder: Option<crate::RelativeFolder>, position: i64 },
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
            list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    )
    SELECT revealed.tab, revealed.watched_folder_path, revealed.group_folder,
        listed.release_position AS position
    FROM import_list_placement AS revealed
    JOIN listed ON listed.release_key = revealed.key
    WHERE revealed.key = :opened AND revealed.item = 'candidate'
"#
);

/// The statement revealing the candidate at `key` from the list `request`
/// shows, whose tab is the candidate's own and whose filters are cleared.
pub(super) fn reveal_params(request: &ImportListRequest, key: &CandidateKey) -> RevealedParams {
    let ListBinding { folds_only, folded, .. } = ListBinding::of(request);
    RevealedParams {
        list_order: order_name(request.view.order),
        tab: None,
        filter: None,
        filter_text: None,
        folds_only,
        folded,
        opened: Some(key.clone()),
    }
}

/// What revealing the candidate at `key` from the list `request` shows: the
/// request on the tab its placement puts it on, the filters cleared, its
/// group opened and the list's order and other folds kept, and where it sits
/// there. `None` for a candidate no tab lists.
pub(super) fn reveal(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
    key: &CandidateKey,
) -> Result<Option<ImportListReveal>, DbError> {
    let Some(row) = statement_optional(sql, &reveal_params(request, key))? else {
        return Ok(None);
    };
    let mut revealed = ImportListRequest {
        view: crate::ImportListView {
            tab: tab_of(&row.tab)?,
            search: None,
            pending_filter: PendingFilter::All,
            order: request.view.order,
        },
        folds: request.folds.clone(),
    };
    if let Some(folder) = row.group_folder {
        let group = crate::FolderReleaseDecisionKey {
            watched_folder_path: path_column::decode(row.watched_folder_path)?,
            relative_folder: folder,
        };
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
        position: to_u64(row.position, "a revealed candidate's position")?,
    }))
}

// The first candidate being identified, Found's first, in the order each
// read names, every tab's rows unfiltered and no group folded.
coven::read!(
    writer NewestFirstIdentifying { key: crate::CandidateKey },
    r#"
    SELECT p.key
    FROM @crate::tables::import_state::listed_fragments::ListedStanding AS st
    JOIN import_list_placement AS p ON p.key = st.standing_candidate AND p.item = 'candidate'
    JOIN import_list_placement AS u ON u.entry = p.unit_entry
    WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick')
    ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END,
        u.sort_queued, u.sort_queue, u.sort_undated, u.sort_newest, u.root_position,
        u.watched_folder_path, u.path_text, u.entry, p.sort_member, p.sort_undated,
        p.sort_newest, p.path_text, p.key, p.entry
    LIMIT 1
"#
);

coven::read!(
    writer OldestFirstIdentifying { key: crate::CandidateKey },
    r#"
    SELECT p.key
    FROM @crate::tables::import_state::listed_fragments::ListedStanding AS st
    JOIN import_list_placement AS p ON p.key = st.standing_candidate AND p.item = 'candidate'
    JOIN import_list_placement AS u ON u.entry = p.unit_entry
    WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick')
    ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END,
        u.sort_queued, u.sort_queue, u.sort_undated, u.sort_oldest, u.root_position,
        u.watched_folder_path, u.path_text, u.entry, p.sort_member, p.sort_undated,
        p.sort_oldest, p.path_text, p.key, p.entry
    LIMIT 1
"#
);

coven::read!(
    writer PathAscendingIdentifying { key: crate::CandidateKey },
    r#"
    SELECT p.key
    FROM @crate::tables::import_state::listed_fragments::ListedStanding AS st
    JOIN import_list_placement AS p ON p.key = st.standing_candidate AND p.item = 'candidate'
    JOIN import_list_placement AS u ON u.entry = p.unit_entry
    WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick')
    ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END,
        u.sort_queued, u.sort_queue, u.root_position, u.watched_folder_path,
        u.path_text, u.entry, p.sort_member, p.path_text, p.key, p.entry
    LIMIT 1
"#
);

coven::read!(
    writer PathDescendingIdentifying { key: crate::CandidateKey },
    r#"
    SELECT p.key
    FROM @crate::tables::import_state::listed_fragments::ListedStanding AS st
    JOIN import_list_placement AS p ON p.key = st.standing_candidate AND p.item = 'candidate'
    JOIN import_list_placement AS u ON u.entry = p.unit_entry
    WHERE st.identification IN ('queued', 'running', 'finalizing', 'reading_pick')
    ORDER BY CASE p.tab WHEN 'pending' THEN 0 WHEN 'done' THEN 1 ELSE 2 END,
        u.sort_queued_back DESC, u.sort_queue_back DESC, u.root_position DESC,
        u.watched_folder_path DESC, u.path_text DESC, u.entry DESC,
        p.sort_header DESC, p.path_text DESC, p.key DESC, p.entry DESC
    LIMIT 1
"#
);

/// The first candidate being identified in the order `order` runs, every
/// tab's rows unfiltered, Found's first: `None` for none.
pub(super) fn first_identifying(
    sql: &impl bae_store::WriterRead,
    order: crate::ImportListOrder,
) -> Result<Option<CandidateKey>, DbError> {
    use crate::ImportListOrder as Order;
    let key = match order {
        Order::NewestFirst => {
            statement_optional(sql, &NewestFirstIdentifyingParams {})?.map(|row| row.key)
        }
        Order::OldestFirst => {
            statement_optional(sql, &OldestFirstIdentifyingParams {})?.map(|row| row.key)
        }
        Order::PathAscending => {
            statement_optional(sql, &PathAscendingIdentifyingParams {})?.map(|row| row.key)
        }
        Order::PathDescending => {
            statement_optional(sql, &PathDescendingIdentifyingParams {})?.map(|row| row.key)
        }
    };
    Ok(key)
}

impl ImportTables {
    /// How many entries each tab holds now.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_tab_counts(&self) -> Result<TriageTabCounts, DbError> {
        let request = ImportListRequest::of(Default::default());
        self.store.read(move |sql| Ok(count(&sql, &request)?.tabs)).await
    }

    /// How many entries each tab holds, live, for a test that waits on a
    /// scan: a screen reads them in its list's count.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_tab_counts(&self) -> coven::LiveQuery<TriageTabCounts, bae_store::DbError> {
        let request = ImportListRequest::of(Default::default());
        subscribe_count(&self.store, request).process(|count| Ok(count.tabs))
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
        self.store.read(move |sql| selection::position_of(&sql, &request, &candidate_key)).await
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
