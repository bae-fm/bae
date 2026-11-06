//! Test stores on a schema and the rows tests seed them with, for the tests of
//! this crate and every crate that builds on it.

/// A store on `schema` in a fresh temp directory, under the real clock.
/// The `TempDir` owns the file, so it must outlive the handle.
pub async fn temp_db(schema: crate::Schema) -> (crate::Store, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = crate::Store::new_test(
        schema,
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// The instant the import fixtures pin their clock to.
pub fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// `temp_db` under that pinned clock instead of the system one.
pub async fn empty_db(schema: crate::Schema) -> (crate::Store, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = crate::Store::new_test(
        schema,
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::FixedClock::new((fixed_now()).into())),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// Run one statement against the test database, panicking if it fails.
pub async fn exec(db: &crate::Store, sql: &str, args: &[&str]) {
    let sql = sql.to_string();
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    db.call(move |conn| {
        conn.execute(&sql, rusqlite::params_from_iter(args))
            .map(|_| ())
            .map_err(crate::DbError::from)
    })
    .await
    .unwrap();
}

/// How many rows `table` holds.
pub async fn row_count(db: &crate::Store, table: &str) -> usize {
    let query = format!("SELECT COUNT(*) FROM \"{table}\"");
    let count: i64 = db
        .read(move |sql| sql.query_row(&query, [], |row| row.get(0)).map_err(crate::DbError::from))
        .await
        .unwrap();
    usize::try_from(count).expect("a row count is never negative")
}

/// `exec` for a multi-statement script, which takes no bound parameters.
pub async fn exec_batch(db: &crate::Store, sql: &str) {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(crate::DbError::from)).await.unwrap();
}

pub mod live_query;
pub mod schema_checks;
pub mod trigger_order;
pub mod window_cost;
