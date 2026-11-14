//! The import list's plain reads: Found's windows and counts, the tab and
//! filter counts, a search's first window, where a row is revealed and a
//! scan's found counts, each worked out from the scan, draft, verdict and
//! library tables every time, with no `import_list_placement` and none of its
//! count tables. A window is a range of the list's order bounded by the keys
//! of the rows at its two ends, and its count is a statement of its own.
//! Every plain number is checked against what the list reads today, then
//! measured. Measurements, not checks; each is ignored and run in release.

use super::cost::{found_view, rows_shown, SIZES};
use super::*;
use bae_store::test_fixtures::screen_cost::{call_cost, plain_cost};
use rusqlite::types::Value;

/// The queue's releases and the headers of Found's groups as placed rows,
/// from `import_list_entry`, which reads the domain tables for one release at
/// a time. `kept` is the condition a row must meet to be shown: `1` for a
/// list with no search. A member sorts in its header's place, so each row
/// carries the date and path of the unit it sorts in (`unit_date`,
/// `unit_path`); a header is the group of its members and holds their newest
/// date. Columns of a header a window never reads (its text, its dates) are
/// NULL.
fn placed(kept: &str) -> String {
    format!(
        "WITH e AS MATERIALIZED (SELECT * FROM import_list_entry), \
         firsts AS MATERIALIZED ( \
            SELECT DISTINCT watched_folder_path AS root, \
                   substr(folder, 1, instr(folder, X'2F') - 1) AS first \
            FROM e WHERE instr(folder, X'2F') > 0), \
         member AS ( \
            SELECT e.watched_folder_path, e.item, e.key, e.root_position, e.tab, \
                   e.pending_state, \
                   CASE WHEN e.pending_state IN ('needs_you', 'lookup_error', 'error', \
                                                 'import_error') THEN 'needs_you' \
                        WHEN e.pending_state IN ('identifying', 'importing') THEN 'in_progress' \
                        ELSE e.pending_state END AS pending_entry, \
                   e.folder, coven_path_text(e.folder) AS display_path, \
                   CASE WHEN e.tab = 'pending' THEN CASE \
                        WHEN instr(e.folder, X'2F') > 0 \
                            THEN substr(e.folder, 1, instr(e.folder, X'2F') - 1) \
                        WHEN EXISTS (SELECT 1 FROM firsts f \
                                     WHERE f.root = e.watched_folder_path \
                                       AND f.first = e.folder) THEN e.folder END END \
                        AS group_folder, \
                   e.own_date, e.shown_title, e.shown_artists, ({kept}) AS kept \
            FROM e), \
         dated AS ( \
            SELECT member.*, coven_path_text(group_folder) AS group_name, \
                   MAX(own_date) OVER (PARTITION BY watched_folder_path, group_folder) \
                       AS group_date \
            FROM member), \
         placed AS ( \
            SELECT watched_folder_path, item, key, root_position, tab, pending_state, \
                   pending_entry, folder, display_path, group_folder, group_name, own_date, \
                   shown_title, shown_artists, \
                   CASE WHEN group_folder IS NULL THEN own_date ELSE group_date END AS unit_date, \
                   COALESCE(group_name, display_path) AS unit_path \
            FROM dated WHERE kept \
            UNION ALL \
            SELECT watched_folder_path, 'group', group_folder, MIN(root_position), 'pending', \
                   NULL, NULL, NULL, NULL, group_folder, group_name, NULL, NULL, NULL, \
                   group_date, group_name \
            FROM dated WHERE group_folder IS NOT NULL \
            GROUP BY watched_folder_path, group_folder HAVING MAX(kept))"
    )
}

/// The columns a window of the list reads.
const WINDOW_COLUMNS: &str = "watched_folder_path, item, key, group_folder, display_path, \
                              shown_title, shown_artists, pending_state, own_date";

/// A list's order as one tuple of ascending terms, one row's key to the
/// next: no term can be NULL, a descending number is negated, and a group
/// leads its members.
struct Order {
    name: &'static str,
    key: &'static str,
    /// How many terms the key has.
    width: usize,
}

const PATH_ASCENDING: Order = Order {
    name: "found-path",
    width: 6,
    key: "root_position, watched_folder_path, unit_path COLLATE natural_order, \
          item <> 'group', COALESCE(display_path, '') COLLATE natural_order, key",
};

const NEWEST_FIRST: Order = Order {
    name: "found-newest",
    width: 10,
    key: "unit_date IS NULL, -COALESCE(unit_date, 0), root_position, watched_folder_path, \
          unit_path COLLATE natural_order, item <> 'group', own_date IS NULL, \
          -COALESCE(own_date, 0), COALESCE(display_path, '') COLLATE natural_order, key",
};

/// The key of the item at `offset` of Found in `order`, or nothing past its
/// end.
async fn key_at(db: &Store, order: &'static Order, offset: u64) -> Option<Vec<Value>> {
    let sql = format!(
        "{} SELECT {} FROM placed WHERE tab = 'pending' ORDER BY {} LIMIT 1 OFFSET ?",
        placed("1"),
        order.key,
        order.key
    );
    db.read(move |sql_read| {
        let rows = sql_read.query(&sql, [offset as i64], |row| {
            (0..order.width).map(|at| row.get::<_, Value>(at)).collect::<Result<Vec<_>, _>>()
        })?;
        Ok(rows.into_iter().next())
    })
    .await
    .unwrap()
}

fn marks(width: usize) -> String {
    vec!["?"; width].join(", ")
}

/// How many items Found holds, which the window's count reads.
async fn found_count(db: &Store, kept: &str, params: Vec<Value>) -> i64 {
    let sql = format!("{} SELECT COUNT(*) FROM placed WHERE tab = 'pending'", placed(kept));
    db.read(move |read| {
        read.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))
            .map_err(Into::into)
    })
    .await
    .unwrap()
}

/// Found's windows in `order`, each as a range bounded by the keys of the rows
/// at its ends, at the top, middle and end of the list and read once by
/// offset beside, and the count: each against its bar. Returns the first
/// window's candidate keys, to check against what the list reads.
async fn found_windows(db: &Store, order: &'static Order, n: u64, size: &str) -> Vec<Vec<u8>> {
    let count = found_count(db, "1", vec![]).await;
    assert!(count > 0);
    let (jump_bar, range_bar, count_bar) =
        if n >= 100_000 { (50.0, 16.0, 100.0) } else { (16.0, 8.0, 50.0) };
    let name = order.name;
    plain_cost(
        db,
        &format!("import/{name}/count@{size}"),
        Some(count_bar),
        &format!("{} SELECT COUNT(*) FROM placed WHERE tab = 'pending'", placed("1")),
        vec![],
    )
    .await;
    let mut first = vec![];
    for (place, offset) in offsets(count as u64) {
        let from = key_at(db, order, offset).await.expect("the window's first row");
        let until = key_at(db, order, offset + LIMIT).await;
        let width = order.width;
        let mut conditions = format!("(({}) >= ({}))", order.key, marks(width));
        let mut params = from;
        if let Some(until) = until {
            conditions.push_str(&format!(" AND (({}) < ({}))", order.key, marks(width)));
            params.extend(until);
        }
        let rows = plain_cost(
            db,
            &format!("import/{name}/range-{place}@{size}"),
            Some(range_bar),
            &format!(
                "{} SELECT {WINDOW_COLUMNS} FROM placed WHERE tab = 'pending' AND {conditions} \
                 ORDER BY {}",
                placed("1"),
                order.key
            ),
            params,
        )
        .await;
        assert!(rows > 0 && rows as u64 <= LIMIT, "{name} {place}: {rows} rows");
        let rows = plain_cost(
            db,
            &format!("import/{name}/jump-{place}@{size}"),
            Some(jump_bar),
            &format!(
                "{} SELECT {WINDOW_COLUMNS} FROM placed WHERE tab = 'pending' \
                 ORDER BY {} LIMIT {LIMIT} OFFSET ?",
                placed("1"),
                order.key
            ),
            vec![Value::Integer(offset as i64)],
        )
        .await;
        assert!(rows > 0, "{name} {place}");
    }
    let sql = format!(
        "{} SELECT item, key FROM placed WHERE tab = 'pending' ORDER BY {} LIMIT {LIMIT}",
        placed("1"),
        order.key
    );
    for (item, key) in db
        .read(move |read| {
            Ok(read
                .query(&sql, [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))?)
        })
        .await
        .unwrap()
    {
        if item == "candidate" {
            first.push(key);
        }
    }
    first
}

/// The tab counts, Found's filter entries and its groups' entry counts, as
/// `GROUP BY` over what each release is placed as; and where a row is
/// revealed, which is the count of the rows before it. Each against its bar.
async fn counts(db: &Store, size: &str) {
    let tabs = plain_cost(
        db,
        &format!("import/tab-counts@{size}"),
        Some(50.0),
        "SELECT tab, COUNT(*), \
                MAX(tab = 'pending' AND instr(folder, X'2F') > 0) AS has_groups \
         FROM import_list_entry GROUP BY tab",
        vec![],
    )
    .await;
    assert!(tabs > 0);
    plain_cost(
        db,
        &format!("import/filter-entries@{size}"),
        Some(50.0),
        "SELECT entry, COUNT(*) FROM ( \
             SELECT CASE WHEN pending_state IN ('needs_you', 'lookup_error', 'error', \
                                                'import_error') THEN 'needs_you' \
                         WHEN pending_state IN ('identifying', 'importing') THEN 'in_progress' \
                         ELSE pending_state END AS entry \
             FROM import_list_entry WHERE tab = 'pending') GROUP BY entry \
         UNION ALL SELECT 'all', COUNT(*) FROM import_list_entry WHERE tab = 'pending'",
        vec![],
    )
    .await;
    plain_cost(
        db,
        &format!("import/group-entry-counts@{size}"),
        Some(50.0),
        &format!(
            "{} SELECT watched_folder_path, group_folder, pending_entry, COUNT(*) \
             FROM dated WHERE group_folder IS NOT NULL \
             GROUP BY watched_folder_path, group_folder, pending_entry",
            placed("1")
        ),
        vec![],
    )
    .await;
}

/// Where a row at the middle of Found is revealed: the count of the items
/// before it.
async fn reveal(db: &Store, size: &str) {
    let count = found_count(db, "1", vec![]).await;
    let key = key_at(db, &PATH_ASCENDING, count as u64 / 2).await.unwrap();
    let rows = plain_cost(
        db,
        &format!("import/reveal@{size}"),
        Some(16.0),
        &format!(
            "{} SELECT COUNT(*) FROM placed WHERE tab = 'pending' AND (({}) < ({}))",
            placed("1"),
            PATH_ASCENDING.key,
            marks(PATH_ASCENDING.width)
        ),
        key,
    )
    .await;
    assert_eq!(rows, 1);
}

/// What typing "Al", "Alb" and "Album 1" costs: the first window of what the
/// search keeps and its count, each a statement over the shown text, against
/// the keystroke's bar.
async fn typed(db: &Store, size: &str) {
    let kept = "bae_fold_case(shown_title) GLOB bae_search_pattern(?1) \
                OR bae_fold_case(shown_artists) GLOB bae_search_pattern(?1)";
    for text in ["Al", "Alb", "Album 1"] {
        let pattern = vec![Value::Text(text.to_string())];
        plain_cost(
            db,
            &format!("import/typed/{}/window@{size}", text.replace(' ', "-")),
            Some(50.0),
            &format!(
                "{} SELECT {WINDOW_COLUMNS} FROM placed WHERE tab = 'pending' \
                 ORDER BY {} LIMIT {LIMIT}",
                placed(kept),
                PATH_ASCENDING.key
            ),
            pattern.clone(),
        )
        .await;
        plain_cost(
            db,
            &format!("import/typed/{}/count@{size}", text.replace(' ', "-")),
            Some(50.0),
            &format!("{} SELECT COUNT(*) FROM placed WHERE tab = 'pending'", placed(kept)),
            pattern,
        )
        .await;
    }
}

/// Each watched folder's scan progress: the folders its current scan holds.
async fn scan_found(db: &Store, size: &str) {
    let rows = plain_cost(
        db,
        &format!("import/scan-found-counts@{size}"),
        Some(2.0),
        "SELECT r.watched_folder_path, \
                (SELECT COUNT(*) FROM scan_candidate c \
                 WHERE c.watched_folder_path = r.watched_folder_path \
                   AND c.generation = r.generation AND c.source_kind = 'folder') \
         FROM folder_scan_roots r",
        vec![],
    )
    .await;
    assert!(rows > 0);
}

/// The sizes to measure: `IMPORT_COST_SIZES`, comma separated, else
/// [`SIZES`].
fn sizes() -> Vec<u64> {
    match std::env::var("IMPORT_COST_SIZES") {
        Ok(list) => list.split(',').map(|n| n.trim().parse().expect("a folder count")).collect(),
        Err(_) => SIZES.to_vec(),
    }
}

/// What the plain reads of the import list cost at each size, each checked
/// against the list's read today and timed beside it (`+today`). Run in
/// release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     import_list_plain_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_plain_cost() {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut held = 0;
    for n in sizes() {
        // In watched folders of 20,000 rows each, as the search scenario
        // grows them: one scan's write passes the rows of its folder.
        for start in (held..n).step_by(20_000) {
            grow_import_list(&db, tmp.path(), start, (start + 20_000).min(n)).await;
        }
        held = n;
        let size = format!("{}k", n / 1_000);
        // What the list reads today is what the plain reads must give.
        let today = tables.load_tab_counts().await.unwrap();
        let plain: (i64, i64, i64) = db
            .read(|read| {
                let rows = read.query(
                    "SELECT tab, COUNT(*) FROM import_list_entry GROUP BY tab",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )?;
                let of = |tab: &str| {
                    rows.iter().find(|(name, _)| name == tab).map(|(_, count)| *count).unwrap_or(0)
                };
                Ok((of("pending"), of("done"), of("skipped")))
            })
            .await
            .unwrap();
        assert_eq!(
            plain,
            (i64::from(today.pending), i64::from(today.done), i64::from(today.skipped)),
            "the plain tab counts"
        );
        let shown = rows_shown(
            &db,
            found_view("", crate::PendingFilter::All, crate::ImportListOrder::PathAscending),
        )
        .await;
        assert_eq!(found_count(&db, "1", vec![]).await as u64, shown, "the plain count");
        call_cost(&format!("import/tab-counts+today@{size}"), 50.0, || tables.load_tab_counts())
            .await;
        counts(&db, &size).await;
        let first = found_windows(&db, &PATH_ASCENDING, n, &size).await;
        let list = tables
            .open_import_list(found_view(
                "",
                crate::PendingFilter::All,
                crate::ImportListOrder::PathAscending,
            ))
            .await
            .unwrap();
        let today_first: Vec<String> = tables
            .read_import_window(
                list,
                bae_store::ListWindow::Forward { from: None, until: None, limit: LIMIT },
            )
            .await
            .unwrap()
            .read
            .rows
            .iter()
            .filter_map(|item| match item {
                crate::ImportListItem::Candidate { row, .. } => Some(row.candidate_key.to_string()),
                _ => None,
            })
            .collect();
        tables.close_import_list(list).await.unwrap();
        let plain_first: Vec<String> =
            first.into_iter().map(|key| crate::CandidateKey::from(key).to_string()).collect();
        assert_eq!(plain_first, today_first, "the plain first window");
        found_windows(&db, &NEWEST_FIRST, n, &size).await;
        reveal(&db, &size).await;
        typed(&db, &size).await;
        scan_found(&db, &size).await;
    }
}
