use super::*;

/// Each column of `table` on `conn`: its name, declared type, whether it is
/// `NOT NULL`, and its place in the primary key.
fn columns(conn: &rusqlite::Connection, table: &str) -> Vec<(String, String, bool, i64)> {
    conn.prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_xinfo(?1) ORDER BY cid")
        .unwrap()
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// A bare library's stand-in for each coven table has the columns of the
/// table coven creates in every library, so the triggers that follow it read
/// and fire on what they do in a library coven opens.
#[tokio::test]
async fn each_stand_in_has_the_columns_of_coven_s_table() {
    let stand_ins = rusqlite::Connection::open_in_memory().unwrap();
    stand_ins.execute_batch(STAND_IN_COVEN_TABLES).unwrap();
    let tables: Vec<String> = stand_ins
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let (store, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let names = tables.clone();
    let created: Vec<String> = store
        .read(move |sql| {
            let mut created = Vec::with_capacity(names.len());
            for name in names {
                created.push(sql.query_row(
                    "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                    [&name],
                    |row| row.get(0),
                )?);
            }
            Ok(created)
        })
        .await
        .unwrap();
    let coven = rusqlite::Connection::open_in_memory().unwrap();
    for table in &created {
        coven.execute_batch(table).unwrap();
    }
    for table in &tables {
        assert_eq!(columns(&stand_ins, table), columns(&coven, table), "{table}");
    }
}
