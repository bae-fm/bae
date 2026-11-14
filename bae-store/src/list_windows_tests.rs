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
