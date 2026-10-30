use super::*;

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
        ListOrder::new(vec![
            SortTerm::ascending("t.a").nullable(),
            SortTerm::ascending("t.id"),
        ]),
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
        .query_map(params.as_slice(), |row| {
            Ok((row.get(0)?, order.key_of(row)?))
        })?
        .collect();
    rows
}

/// Every window of five rows, read from the row before it and from the row
/// after it, holds the rows reading it by its offset holds, in every order.
#[test]
fn a_window_read_from_either_neighbour_holds_its_offsets_rows() {
    let conn = rows();
    for order in orders() {
        let everything = read(&conn, &order, &order.everything(), 1_000, 0).unwrap();
        assert_eq!(everything.len(), 200);
        for offset in 0..200_u64 {
            let window = LibraryPageWindow { offset, limit: 5 };
            let expected = everything
                .iter()
                .skip(offset as usize)
                .take(5)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let mut neighbours = Vec::new();
            if offset > 0 {
                neighbours.push(Places::from([(
                    offset - 1,
                    everything[offset as usize - 1].1.clone(),
                )]));
            }
            if let Some((_, key)) = everything.get(offset as usize + 5) {
                neighbours.push(Places::from([(offset + 5, key.clone())]));
            }
            for known in neighbours {
                let WindowsRead { windows, .. } = read_windows(
                    &order,
                    &LibraryPageWindows::from([window.clone()]),
                    known.clone(),
                    |seek, limit, offset| read(&conn, &order, seek, limit, offset),
                )
                .unwrap();
                assert_eq!(
                    windows[0].1, expected,
                    "{order:?} at {offset} from {known:?}"
                );
            }
        }
    }
}

/// Windows read together start each from the one before it, and the read
/// gives back where each window's first and last rows sit.
#[test]
fn adjacent_windows_read_together_chain_and_report_their_ends() {
    let conn = rows();
    let order = orders().pop().unwrap();
    let everything = read(&conn, &order, &order.everything(), 1_000, 0).unwrap();
    let windows = LibraryPageWindows::from([
        LibraryPageWindow {
            offset: 50,
            limit: 10,
        },
        LibraryPageWindow {
            offset: 60,
            limit: 10,
        },
        LibraryPageWindow {
            offset: 195,
            limit: 10,
        },
    ]);
    let mut offsets_read = Vec::new();
    let WindowsRead {
        windows: answered,
        found: places,
    } = read_windows(&order, &windows, Places::new(), |seek, limit, offset| {
        if seek.reads_everything() {
            offsets_read.push(offset);
        }
        read(&conn, &order, seek, limit, offset)
    })
    .unwrap();
    assert_eq!(offsets_read, vec![50, 195]);
    for (window, ids) in &answered {
        let expected = everything
            .iter()
            .skip(window.offset as usize)
            .take(window.limit as usize)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, &expected, "{window:?}");
    }
    assert_eq!(
        places.keys().copied().collect::<Vec<_>>(),
        vec![50, 59, 60, 69, 195, 199]
    );
    for (place, key) in &places {
        assert_eq!(key, &everything[*place as usize].1);
    }
}

/// Places a read kept are handed to the next read only while the list stands
/// as it stood for that read.
#[test]
fn places_kept_under_one_standing_are_none_under_another() {
    let known = KnownPlaces::new();
    let places = Places::from([(3, OrderKey(vec![Value::Integer(1)]))]);
    known.keep(vec![1_i64, 2], places.clone());
    assert_eq!(known.under(&vec![1, 2]), places);
    assert!(known.under(&vec![1, 3]).is_empty());
}
