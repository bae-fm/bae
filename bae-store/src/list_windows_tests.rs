use super::*;
use rusqlite::Connection;

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

/// SQLite's NOCASE, which two of the rows' `b` values ('x' and 'X') are
/// equal under.
fn nocase(left: &str, right: &str) -> Ordering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

fn orders() -> Vec<ListOrder> {
    vec![
        ListOrder::new(vec![SortTerm::ascending("t.id")]),
        ListOrder::new(vec![SortTerm::descending("t.id")]),
        ListOrder::new(vec![SortTerm::ascending("t.a").nullable(), SortTerm::ascending("t.id")]),
        ListOrder::new(vec![
            SortTerm::descending("t.a").nullable(),
            SortTerm::ascending("t.b COLLATE NOCASE").collated(nocase),
            SortTerm::ascending("t.id"),
        ]),
        ListOrder::new(vec![
            SortTerm::ascending("t.a IS NULL"),
            SortTerm::descending("t.a").nullable(),
            SortTerm::descending("t.b COLLATE NOCASE").collated(nocase),
            SortTerm::ascending("t.c"),
            SortTerm::descending("t.id"),
        ]),
    ]
}

/// `seek`'s rows of `t`, at most `limit` from `offset`, each with its key.
fn read(
    conn: &Connection,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> rusqlite::Result<Vec<Keyed<String>>> {
    let statement = format!(
        "SELECT t.id, {keys} FROM t WHERE {condition} ORDER BY {order} \
         LIMIT :limit OFFSET :offset",
        keys = order.key_columns(),
        condition = order.condition(seek),
        order = order.ordering(seek),
    );
    let (limit, offset) = (limit as i64, offset as i64);
    let mut params = seek.params();
    params.push((":limit", &limit));
    params.push((":offset", &offset));
    let mut statement = conn.prepare(&statement)?;
    let rows = statement
        .query_map(params.as_slice(), |row| Ok((row.get(0)?, order.key_of(row)?)))?
        .collect();
    rows
}

/// Every window of five rows, read from a cursor at the row before it and at
/// the row after it, holds the rows reading it by its offset holds, in every
/// order, and reports the keys of its first and last rows.
#[test]
fn a_window_read_from_a_cursor_holds_its_offsets_rows() {
    let conn = rows();
    for order in orders() {
        let everything = read(&conn, &order, &order.everything(), 1_000, 0).unwrap();
        assert_eq!(everything.len(), 200);
        for offset in 0..200_u64 {
            let expected = everything
                .iter()
                .skip(offset as usize)
                .take(5)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let mut cursors = vec![LibraryPageWindow::at(offset, 5)];
            if offset > 0 {
                let before = everything[offset as usize - 1].1.clone();
                cursors.push(LibraryPageWindow::from_cursor(offset, 5, Cursor::After(before)));
            }
            if let Some((_, key)) = everything.get(offset as usize + 5) {
                cursors.push(LibraryPageWindow::from_cursor(
                    offset,
                    5,
                    Cursor::Before(key.clone()),
                ));
            }
            for window in cursors {
                let read = read_windows(
                    &order,
                    &LibraryPageWindows::from([window.clone()]),
                    |seek, limit, offset| read(&conn, &order, seek, limit, offset),
                )
                .unwrap();
                assert_eq!(read[0].rows, expected, "{order:?} at {offset} from {window:?}");
                let at = offset as usize;
                assert_eq!(read[0].first.as_ref(), Some(&everything[at].1));
                assert_eq!(read[0].last.as_ref(), Some(&everything[(at + 4).min(199)].1));
            }
        }
    }
}

/// A window with a cursor never reads by offset: every statement it runs is a
/// seek past the cursor's row.
#[test]
fn a_window_from_a_cursor_reads_only_past_its_row() {
    let conn = rows();
    let order = orders().pop().unwrap();
    let everything = read(&conn, &order, &order.everything(), 1_000, 0).unwrap();
    let windows = LibraryPageWindows::from([LibraryPageWindow::from_cursor(
        60,
        10,
        Cursor::After(everything[59].1.clone()),
    )]);
    let mut offsets_read = Vec::new();
    read_windows(&order, &windows, |seek, limit, offset| {
        if seek.reads_everything() {
            offsets_read.push(offset);
        }
        read(&conn, &order, seek, limit, offset)
    })
    .unwrap();
    assert!(offsets_read.is_empty());
}

/// A window past the list's end, or of no rows, holds none.
#[test]
fn a_window_past_the_lists_end_holds_no_rows() {
    let conn = rows();
    let order = orders().into_iter().next().unwrap();
    let windows =
        LibraryPageWindows::from([LibraryPageWindow::at(500, 6), LibraryPageWindow::at(0, 0)]);
    let read = read_windows(&order, &windows, |seek, limit, offset| {
        read(&conn, &order, seek, limit, offset)
    })
    .unwrap();
    assert!(read.iter().all(|window| window.rows.is_empty() && window.first.is_none()));
}

/// Keys of a window set compare by their values, so a window moved to another
/// cursor is another window.
#[test]
fn windows_with_different_cursors_are_different_windows() {
    let key = |value| OrderKey::new(vec![SortValue::Integer(value), SortValue::Real(1.5)]);
    let windows = LibraryPageWindows::from([
        LibraryPageWindow::from_cursor(5, 5, Cursor::After(key(1))),
        LibraryPageWindow::from_cursor(5, 5, Cursor::After(key(2))),
        LibraryPageWindow::from_cursor(5, 5, Cursor::Before(key(1))),
        LibraryPageWindow::at(5, 5),
        LibraryPageWindow::from_cursor(5, 5, Cursor::After(key(1))),
    ]);
    assert_eq!(windows.len(), 4);
}

/// The rows of every order, in it, with each row's key.
fn listed(conn: &Connection, order: &ListOrder) -> Vec<Keyed<String>> {
    read(conn, order, &order.everything(), 1_000, 0).unwrap()
}

/// A key that no row has and that the order places right after `row`'s key
/// when its last term runs up, and right before it when it runs down.
fn phantom_beside(order: &ListOrder, row: &OrderKey) -> OrderKey {
    let mut values = row.values().to_vec();
    let Some(SortValue::Text(id)) = values.last_mut() else { panic!("ids are text") };
    id.push('~');
    assert!(order.term_count() == values.len());
    OrderKey::new(values)
}

fn ids(rows: &[Keyed<String>]) -> Vec<String> {
    rows.iter().map(|(id, _)| id.clone()).collect()
}

/// A forward window reads exactly the rows of `[from, until)`: every bound
/// absent, a row's key, or a key no row has, in every order, with its ties,
/// its NULLs and its terms of a collation.
#[test]
fn a_range_holds_exactly_the_rows_between_its_bounds() {
    let conn = rows();
    for order in orders() {
        let everything = listed(&conn, &order);
        let last_runs_down = order.terms.last().unwrap().descending;
        // A bound with the position it falls on: the boundary before that row.
        let mut bounds: Vec<(Option<OrderKey>, usize)> = vec![(None, usize::MAX)];
        for at in [0, 1, 2, 3, 49, 50, 51, 99, 100, 150, 198, 199] {
            bounds.push((Some(everything[at].1.clone()), at));
            let phantom = phantom_beside(&order, &everything[at].1);
            bounds.push((Some(phantom), if last_runs_down { at } else { at + 1 }));
        }
        for (from, from_at) in &bounds {
            for (until, until_at) in &bounds {
                let start = if from.is_some() { *from_at } else { 0 };
                let end = if until.is_some() { *until_at } else { everything.len() };
                let expected = if start < end { ids(&everything[start..end]) } else { Vec::new() };
                let window =
                    ListWindow::Forward { from: from.clone(), until: until.clone(), limit: 1_000 };
                let read = read_range(&order, &window, &mut |seek, limit, offset| {
                    read(&conn, &order, seek, limit, offset)
                })
                .unwrap();
                assert_eq!(read.rows, expected, "{order:?} from {from_at} until {until_at}");
                assert_eq!(read.beyond, None);
                let first = everything.get(start).filter(|_| start < end).map(|row| &row.1);
                assert_eq!(read.first.as_ref(), first);
            }
        }
    }
}

/// A window the limit cuts short names the key of the row after its last
/// one; one that reaches its end bound, the list's end or the list's top
/// names none. A backward window reads the rows nearest before its bound and
/// delivers them in the list's order.
#[test]
fn a_window_cut_by_its_limit_names_the_next_row() {
    let conn = rows();
    for order in orders() {
        let everything = listed(&conn, &order);
        let read = |window: &ListWindow| {
            read_range(&order, window, &mut |seek, limit, offset| {
                read(&conn, &order, seek, limit, offset)
            })
            .unwrap()
        };
        let key = |at: usize| Some(everything[at].1.clone());

        let cut = read(&ListWindow::Forward { from: key(20), until: key(60), limit: 10 });
        assert_eq!(cut.rows, ids(&everything[20..30]));
        assert_eq!(cut.beyond, key(30));

        let exact = read(&ListWindow::Forward { from: key(20), until: key(30), limit: 10 });
        assert_eq!(exact.rows, ids(&everything[20..30]));
        assert_eq!(exact.beyond, None, "the window reached its end bound with its limit");

        let to_the_end = read(&ListWindow::Forward { from: key(195), until: None, limit: 10 });
        assert_eq!(to_the_end.rows, ids(&everything[195..]));
        assert_eq!(to_the_end.beyond, None);

        let top = read(&ListWindow::Forward { from: None, until: None, limit: 7 });
        assert_eq!(top.rows, ids(&everything[..7]));
        assert_eq!(top.first, key(0));
        assert_eq!(top.beyond, key(7));

        let back = read(&ListWindow::Backward { until: everything[40].1.clone(), limit: 10 });
        assert_eq!(back.rows, ids(&everything[30..40]));
        assert_eq!(back.first, key(30));
        assert_eq!(back.beyond, key(29));

        let to_the_top = read(&ListWindow::Backward { until: everything[3].1.clone(), limit: 10 });
        assert_eq!(to_the_top.rows, ids(&everything[..3]));
        assert_eq!(to_the_top.beyond, None);

        let empty = read(&ListWindow::Forward { from: key(50), until: key(50), limit: 10 });
        assert!(empty.rows.is_empty() && empty.first.is_none() && empty.beyond.is_none());
    }
}

/// `rows_before` counts the rows ahead of a key through the order's seeks,
/// and `key_at` reads the key at a position by offset, the last row's when
/// the position is past the end.
#[test]
fn a_position_is_counted_before_a_key_and_read_as_a_key() {
    let conn = rows();
    for order in orders() {
        let everything = listed(&conn, &order);
        for at in [0, 1, 77, 199] {
            let count = rows_before(&order, &everything[at].1, |seek| {
                let statement = format!("SELECT COUNT(*) FROM t WHERE {}", order.condition(seek));
                conn.query_row(&statement, seek.params().as_slice(), |row| row.get::<_, i64>(0))
                    .map(|count| count as u64)
            })
            .unwrap();
            assert_eq!(count, at as u64, "{order:?}");
        }
        let key_at = |position| {
            key_at(&order, position, &mut |seek, limit, offset| {
                read(&conn, &order, seek, limit, offset)
            })
            .unwrap()
        };
        assert_eq!(key_at(0).as_ref(), Some(&everything[0].1));
        assert_eq!(key_at(120).as_ref(), Some(&everything[120].1));
        assert_eq!(key_at(5_000).as_ref(), Some(&everything[199].1));
    }
}

/// Every seek of a range bounded at both ends is one search of the order's
/// index, with the terms it holds equal and the bounds it compares in the
/// search, and no temp b-tree.
#[test]
fn every_seek_of_a_two_bound_range_searches_an_index() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE t (id TEXT PRIMARY KEY, a INTEGER, b TEXT NOT NULL, c INTEGER NOT NULL);
         CREATE INDEX t_by ON t (a, b COLLATE NOCASE, c, id);
         INSERT INTO t VALUES ('id1', 1, 'x', 1), ('id2', 2, 'y', 2), ('id3', 3, 'z', 3);",
    )
    .unwrap();
    let order = ListOrder::new(vec![
        SortTerm::ascending("t.a"),
        SortTerm::ascending("t.b COLLATE NOCASE").collated(nocase),
        SortTerm::ascending("t.c"),
        SortTerm::ascending("t.id"),
    ]);
    let key = |a: i64, b: &str, c: i64, id: &str| {
        OrderKey::new(vec![
            SortValue::Integer(a),
            SortValue::Text(b.into()),
            SortValue::Integer(c),
            SortValue::Text(id.into()),
        ])
    };
    // Bounds that differ at each term in turn.
    for (from, until) in [
        (key(1, "x", 1, "id1"), key(2, "x", 1, "id1")),
        (key(1, "x", 1, "id1"), key(1, "y", 1, "id1")),
        (key(1, "x", 1, "id1"), key(1, "x", 2, "id1")),
        (key(1, "x", 1, "id1"), key(1, "x", 1, "id2")),
    ] {
        let seeks = order.range_seeks(Some(&from), Some(&until));
        assert!(!seeks.is_empty());
        for seek in seeks {
            let statement = format!(
                "EXPLAIN QUERY PLAN SELECT t.id FROM t WHERE {} ORDER BY {} LIMIT 10",
                order.condition(&seek),
                order.ordering(&seek),
            );
            let mut prepared = conn.prepare(&statement).unwrap();
            let steps: Vec<String> = prepared
                .query_map(seek.params().as_slice(), |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(steps.len(), 1, "{statement}: {steps:?}");
            assert!(steps[0].starts_with("SEARCH t USING"), "{steps:?}");
            let searched = steps[0].split_once('(').expect("the search names its terms").1;
            assert_eq!(searched.matches("=?").count(), seek.held_equal(), "{steps:?}");
            let compared = searched.matches(">?").count() + searched.matches("<?").count();
            assert_eq!(compared, if seek.bounds_both_ends() { 2 } else { 1 }, "{steps:?}");
        }
    }
}

/// Two live queries over neighbouring ranges of one list: a write inside a
/// range reruns that window alone, and a write above, below or past both
/// reruns neither, not even the window whose bound is the row's neighbour.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_reruns_only_the_window_whose_range_holds_it() {
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
    let window = |from: i64, until: i64| ListWindow::Forward {
        from: Some(key(from)),
        until: Some(key(until)),
        limit: 100,
    };
    let subscribe = |window: ListWindow| {
        let rows_order = order.clone();
        store.subscribe_list_window(window, order.clone(), move |sql, seek, limit, offset| {
            let statement = format!(
                "SELECT t.id, {keys} FROM t WHERE {condition} ORDER BY {ordering} \
                 LIMIT :limit OFFSET :offset",
                keys = rows_order.key_columns(),
                condition = rows_order.condition(seek),
                ordering = rows_order.ordering(seek),
            );
            seek.bound(limit, offset, |params| {
                sql.query(&statement, params, |row| {
                    Ok((row.get::<_, String>(0)?, rows_order.key_of(row)?))
                })
            })
            .map_err(crate::DbError::from)
        })
    };
    let mut lower = subscribe(window(100, 300));
    let mut upper = subscribe(window(300, 600));
    for query in [&mut lower, &mut upper] {
        tokio::time::timeout(HANG_GUARD, query.next()).await.unwrap().unwrap();
    }
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

    // Inside the upper range, and at its bound: the row after the lower
    // range's last, which the upper range holds.
    for (n, id) in [(455, "x455"), (300, "id300a")] {
        insert(n, id).await;
        assert!(upper.is_marked_for_rerun(), "{id} is inside the upper range");
        assert!(!lower.is_marked_for_rerun(), "{id} is past the lower range's end");
        let event = tokio::time::timeout(HANG_GUARD, upper.next()).await.unwrap().unwrap();
        assert!(event.result.unwrap().rows.contains(&id.to_string()));
    }
    // Inside the lower range.
    insert(205, "x205").await;
    assert!(lower.is_marked_for_rerun() && !upper.is_marked_for_rerun());
    tokio::time::timeout(HANG_GUARD, lower.next()).await.unwrap().unwrap();
    // Above both, past both, and between the two windows' bounds' neighbours.
    for (n, id) in [(5, "x005"), (950, "x950"), (99, "x099")] {
        insert(n, id).await;
        assert!(!lower.is_marked_for_rerun() && !upper.is_marked_for_rerun(), "{id}");
    }
}
