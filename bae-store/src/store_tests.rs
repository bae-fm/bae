use crate::{DbError, Schema};

#[tokio::test]
async fn processing_runs_off_the_runtime_and_preserves_database_errors() {
    let (db, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let runtime_thread = std::thread::current().id();
    let fetched_thread = db
        .read(|sql| {
            let value = sql.query_row("SELECT 42", [], |row| row.get::<_, i64>(0))?;
            Ok((value, std::thread::current().id()))
        })
        .process(move |(value, reader)| {
            assert_eq!(value, 42);
            assert_ne!(runtime_thread, std::thread::current().id());
            Ok(reader)
        })
        .await
        .unwrap();
    assert_ne!(fetched_thread, std::thread::current().id());

    let error = db
        .read(|sql| Ok(sql.query_row("SELECT 1 WHERE 0", [], |row| row.get::<_, i64>(0))?))
        .process(|_| -> Result<(), DbError> { panic!("a failed fetch must skip processing") })
        .await
        .unwrap_err();
    assert!(matches!(error, DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)));

    let error = db
        .read(|_| Ok(()))
        .process(|()| Err::<(), _>(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)))
        .await
        .unwrap_err();
    assert!(matches!(error, DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)));
}
