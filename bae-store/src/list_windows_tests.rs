use super::*;
use rusqlite::{Connection, ToSql};

/// Rows with ties on every term but the id, and NULLs in the terms that can
/// hold them, so each seek of every order below has rows to find.
fn rows() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE t (id TEXT PRIMARY KEY, a INTEGER, b TEXT NOT NULL, c INTEGER NOT NULL);
         WITH RECURSIVE k(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM k WHERE i < 199)
         INSERT INTO t (id, a, b, c)
         SELECT printf('id%03d', (i * 37) % 200),
                CASE WHEN i % 5 = 0 THEN NULL ELSE i % 4 END,
                CASE i % 3 WHEN 0 THEN 'x' WHEN 1 THEN 'X' ELSE 'y' END,
                i % 2
         FROM k;",
    )
    .unwrap();
    conn
}

fn orders() -> Vec<ListOrder> {
    vec![
        ListOrder::new(vec![SortTerm::ascending("t.id")]),
        ListOrder::new(vec![SortTerm::descending("t.id")]),
        ListOrder::new(vec![SortTerm::ascending("t.a").nullable(), SortTerm::ascending("t.id")]),
        ListOrder::new(vec![
            SortTerm::descending("t.a").nullable(),
            SortTerm::ascending("t.b COLLATE NOCASE"),
            SortTerm::ascending("t.id"),
        ]),
        ListOrder::new(vec![
            SortTerm::ascending("t.a IS NULL"),
            SortTerm::descending("t.a").nullable(),
            SortTerm::descending("t.b COLLATE NOCASE"),
            SortTerm::ascending("t.c"),
            SortTerm::descending("t.id"),
        ]),
    ]
}

/// `seek`'s rows of `t`, at most `limit`, each with its key.
fn read(
    conn: &Connection,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> rusqlite::Result<Vec<Keyed<String>>> {
    let mut statement = conn.prepare(&statement(order, seek))?;
    seek.bound(limit, |params| {
        statement.query_map(params, |row| Ok((row.get(0)?, order.key_of(row)?)))?.collect()
    })
}

/// The statement reading `t`'s rows `seek` holds, in `order`, each with its
/// key.
fn statement(order: &ListOrder, seek: &Seek) -> String {
    format!(
        "SELECT t.id, {keys} FROM t WHERE {condition} ORDER BY {order} {limit}",
        keys = order.key_columns(),
        condition = order.condition(seek),
        order = order.ordering(seek),
        limit = seek.limit_clause(),
    )
}

/// Every row of `t` in `order`, with its key.
fn listed(conn: &Connection, order: &ListOrder) -> Vec<Keyed<String>> {
    read(conn, order, &order.everything(), 1_000).unwrap()
}

/// A key that no row has and that the order places right after `row`'s key
/// when its last term sorts ascending, and right before it when it sorts
/// descending.
fn phantom_beside(order: &ListOrder, row: &OrderKey) -> OrderKey {
    let mut values = row.values().to_vec();
    let Some(SortValue::Text(id)) = values.last_mut() else { panic!("ids are text") };
    id.push('~');
    assert!(order.term_count() == values.len());
    OrderKey::new(values)
}

/// A window reads exactly the `limit` rows at or after its key, or from the
/// list's top: a row's key, or a key no row has, in every order, with its
/// ties, its NULLs and its terms of a collation, and each row comes with its
/// key.
#[test]
fn a_window_holds_the_rows_from_its_key() {
    let conn = rows();
    for order in orders() {
        let everything = listed(&conn, &order);
        let last_runs_down = order.terms.last().unwrap().descending;
        // A key with the position of the first row at or after it.
        let mut froms: Vec<(Option<OrderKey>, usize)> = vec![(None, 0)];
        for at in [0, 1, 2, 3, 49, 50, 51, 99, 100, 150, 198, 199] {
            froms.push((Some(everything[at].1.clone()), at));
            let phantom = phantom_beside(&order, &everything[at].1);
            froms.push((Some(phantom), if last_runs_down { at } else { at + 1 }));
        }
        for (from, start) in &froms {
            for limit in [1, 10, 1_000] {
                let end = (start + limit as usize).min(everything.len());
                let window = ListWindow { from: from.clone(), limit };
                let read = read_range(&order, &window, &mut |seek, limit| {
                    read(&conn, &order, seek, limit)
                })
                .unwrap();
                assert_eq!(read.window, window);
                assert_eq!(read.rows, everything[*start..end], "{order:?} from {start}");
            }
        }
    }
}

/// One `RowsBefore` statement counts the rows ahead of a key through the
/// order's seeks, a filter narrows what it counts, and `key_at` reads the
/// key at a position by offset, the last row's when the position is past
/// the end.
#[test]
fn a_position_is_counted_before_a_key_and_read_as_a_key() {
    let conn = rows();
    let count = |before: RowsBefore, params: &[(&str, &dyn ToSql)]| {
        let mut bound = before.params();
        bound.extend_from_slice(params);
        conn.query_row(&before.statement, bound.as_slice(), |row| row.get::<_, i64>(0)).unwrap()
    };
    let odd_ids: std::collections::HashSet<String> = conn
        .prepare("SELECT id FROM t WHERE c = 1")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for order in orders() {
        let everything = listed(&conn, &order);
        for at in [0, 1, 77, 199] {
            let key = &everything[at].1;
            let before = RowsBefore::new(&order, key, "FROM t", None);
            assert_eq!(count(before, &[]), at as i64, "{order:?}");
            let odd = everything[..at].iter().filter(|(id, _)| odd_ids.contains(id)).count();
            let before = RowsBefore::new(&order, key, "FROM t", Some("t.c = :c"));
            assert_eq!(count(before, &[(":c", &1)]), odd as i64, "{order:?}");
        }
        let key_at = |position| {
            key_at(&order, position, &mut |seek, limit| read(&conn, &order, seek, limit)).unwrap()
        };
        assert_eq!(key_at(0).as_ref(), Some(&everything[0].1));
        assert_eq!(key_at(120).as_ref(), Some(&everything[120].1));
        assert_eq!(key_at(5_000).as_ref(), Some(&everything[199].1));
    }
}

/// Every seek of a window from a key is one search of the order's index,
/// with the terms it holds equal and the one it compares in the search, and
/// no temp b-tree.
#[test]
fn every_seek_of_a_window_searches_an_index() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE t (id TEXT PRIMARY KEY, a INTEGER, b TEXT NOT NULL, c INTEGER NOT NULL);
         CREATE INDEX t_by ON t (a, b COLLATE NOCASE, c, id);
         INSERT INTO t VALUES ('id1', 1, 'x', 1), ('id2', 2, 'y', 2), ('id3', 3, 'z', 3);",
    )
    .unwrap();
    let order = ListOrder::new(vec![
        SortTerm::ascending("t.a"),
        SortTerm::ascending("t.b COLLATE NOCASE"),
        SortTerm::ascending("t.c"),
        SortTerm::ascending("t.id"),
    ]);
    let from = OrderKey::new(vec![
        SortValue::Integer(1),
        SortValue::Text("x".into()),
        SortValue::Integer(1),
        SortValue::Text("id1".into()),
    ]);
    let seeks = order.window_seeks(Some(&from));
    assert_eq!(seeks.len(), 4, "one seek past each term");
    for seek in seeks {
        let statement = format!(
            "EXPLAIN QUERY PLAN SELECT t.id FROM t WHERE {} ORDER BY {} {}",
            order.condition(&seek),
            order.ordering(&seek),
            seek.limit_clause(),
        );
        let mut prepared = conn.prepare(&statement).unwrap();
        let steps: Vec<String> = seek
            .bound(10, |params| {
                prepared.query_map(params, |row| row.get::<_, String>(3))?.collect::<Result<_, _>>()
            })
            .unwrap();
        assert_eq!(steps.len(), 1, "{statement}: {steps:?}");
        assert!(steps[0].starts_with("SEARCH t USING"), "{steps:?}");
        let searched = steps[0].split_once('(').expect("the search names its terms").1;
        assert_eq!(searched.matches("=?").count(), seek.held_equal(), "{steps:?}");
        let compared = searched.matches(">?").count() + searched.matches("<?").count();
        assert_eq!(compared, 1, "{steps:?}");
    }
}

/// A window's statements' reads, each with its key: what a list's live
/// query runs for each seek.
fn read_live(
    sql: &coven::SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<String>>, crate::DbError> {
    let statement = format!(
        "SELECT t.id, {keys} FROM t WHERE {condition} ORDER BY {ordering} {limit}",
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    );
    seek.bound(limit, |params| {
        sql.query(&statement, params, |row| Ok((row.get::<_, String>(0)?, order.key_of(row)?)))
    })
    .map_err(crate::DbError::from)
}

/// A window from a key, cut by its limit: a write at or before its last
/// row reruns it, and a write before its key or after its last row reruns
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_reruns_the_window_only_up_to_its_last_row() {
    use bae_util::test_channels::HANG_GUARD;
    let (store, _tmp) = crate::test_fixtures::temp_db(
        crate::Schema::new(&[]).followed_by(
            "CREATE TABLE t (id TEXT PRIMARY KEY NOT NULL, n INTEGER NOT NULL); \
         CREATE INDEX t_by ON t (n, id);"
                .to_string(),
        ),
    )
    .await;
    crate::test_fixtures::exec_batch(
        &store,
        "WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 89) \
         INSERT INTO t (id, n) SELECT printf('id%03d', i * 10), i * 10 FROM k",
    )
    .await;
    let order = ListOrder::new(vec![SortTerm::ascending("t.n"), SortTerm::ascending("t.id")]);
    let key =
        |n: i64| OrderKey::new(vec![SortValue::Integer(n), SortValue::Text(format!("id{n:03}"))]);
    // The rows from 100 to 300.
    let rows_order = order.clone();
    let mut window = store.subscribe_list_window(
        ListWindow { from: Some(key(100)), limit: 21 },
        order,
        move |sql, seek, limit| read_live(sql, &rows_order, seek, limit),
    );
    let first = tokio::time::timeout(HANG_GUARD, window.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap().rows.last().unwrap().0, "id300");
    let insert = |n: i64, id: &'static str| {
        let store = &store;
        async move {
            crate::test_fixtures::exec(
                store,
                "INSERT INTO t (id, n) VALUES (?1, ?2)",
                &[id, &n.to_string()],
            )
            .await;
        }
    };

    // Inside the window: its last row is then x285.
    for (n, id) in [(205, "x205"), (285, "x285")] {
        insert(n, id).await;
        assert!(window.is_marked_for_rerun(), "{id} is inside the window");
        let event = tokio::time::timeout(HANG_GUARD, window.next_inline()).await.unwrap().unwrap();
        assert!(event.result.unwrap().rows.iter().any(|(row, _)| row == id));
    }
    // Before its key, and after its last row, one of them tying its value.
    for (n, id) in [(5, "x005"), (99, "x099"), (285, "x285a"), (455, "x455"), (950, "x950")] {
        insert(n, id).await;
        assert!(!window.is_marked_for_rerun(), "{id} is outside the window");
    }
}

/// Windows over a list ordered by a nullable value, whose NULLs sort first:
/// one holds NULLs only, and one crosses from the NULLs into the values. A
/// write reruns the window whose rows run up to its row, whether the row's
/// value is NULL or not, and no window that ends before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_reruns_only_the_window_over_the_nulls_or_the_values_that_reach_it() {
    use bae_util::test_channels::HANG_GUARD;
    let (store, _tmp) = crate::test_fixtures::temp_db(
        crate::Schema::new(&[]).followed_by(
            "CREATE TABLE t (id TEXT PRIMARY KEY NOT NULL, n INTEGER); \
         CREATE INDEX t_by ON t (n, id);"
                .to_string(),
        ),
    )
    .await;
    crate::test_fixtures::exec_batch(
        &store,
        "INSERT INTO t (id, n) VALUES ('n010', NULL), ('n020', NULL), ('n030', NULL), \
         ('n040', NULL), ('v100', 100), ('v200', 200), ('v300', 300), ('v400', 400)",
    )
    .await;
    let order =
        ListOrder::new(vec![SortTerm::ascending("t.n").nullable(), SortTerm::ascending("t.id")]);
    let null_key = |id: &str| OrderKey::new(vec![SortValue::Null, SortValue::Text(id.to_string())]);
    let subscribe = |window: ListWindow| {
        let rows_order = order.clone();
        store.subscribe_list_window(window, order.clone(), move |sql, seek, limit| {
            read_live(sql, &rows_order, seek, limit)
        })
    };
    // The first window holds n010 to n030, the second n030 to v200.
    let mut windows = [
        subscribe(ListWindow { from: None, limit: 3 }),
        subscribe(ListWindow { from: Some(null_key("n030")), limit: 4 }),
    ];
    for query in &mut windows {
        tokio::time::timeout(HANG_GUARD, query.next_inline()).await.unwrap().unwrap();
    }
    let write = |sql: &'static str| {
        let store = &store;
        async move { crate::test_fixtures::exec_batch(store, sql).await }
    };

    // Each write reruns only the windows whose rows run up to the row before
    // or after the write.
    let writes: [(&'static str, [bool; 2]); 5] = [
        ("INSERT INTO t (id, n) VALUES ('n015', NULL)", [true, false]),
        ("INSERT INTO t (id, n) VALUES ('n035', NULL)", [false, true]),
        ("INSERT INTO t (id, n) VALUES ('v050', 50)", [false, true]),
        ("INSERT INTO t (id, n) VALUES ('v150', 150)", [false, false]),
        // A row moving from the NULLs into the values past both windows.
        ("UPDATE t SET n = 500 WHERE id = 'n020'", [true, false]),
    ];
    for (sql, expected) in writes {
        write(sql).await;
        let marked: Vec<bool> = windows.iter().map(|query| query.is_marked_for_rerun()).collect();
        assert_eq!(marked, expected, "{sql}");
        for (query, marked) in windows.iter_mut().zip(expected) {
            if marked {
                tokio::time::timeout(HANG_GUARD, query.next_inline()).await.unwrap().unwrap();
            }
        }
    }
}

/// A page's one statement reads exactly the `limit` rows from the row at its
/// first place, in every order, with its ties, its NULLs and its terms of a
/// collation, and none for a page past the list's end.
#[test]
fn a_page_holds_the_rows_from_its_first_place() {
    let conn = rows();
    for order in orders() {
        let everything = listed(&conn, &order);
        for place in [0, 1, 2, 39, 40, 41, 99, 150, 195, 199, 200, 250] {
            let seek = order.from_place(place, statement);
            let read = read(&conn, &order, &seek, 7).unwrap();
            let start = (place as usize).min(everything.len());
            let end = (start + 7).min(everything.len());
            assert_eq!(read, everything[start..end], "page at {place} in {order:?}");
        }
    }
}
