use super::super::*;
use super::*;

#[tokio::test]
async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
    let (db, _tmp) = super::temp_db().await;

    let track = DbTrack::new_test("missing-release", TRACK_A, "Track Title A", Some(1));
    let error = db
        .insert_track(&track)
        .await
        .expect_err("track insert without a release must violate the foreign key");

    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "expected a foreign-key violation, got {error}"
    );
}

#[tokio::test]
async fn processing_uses_separate_workers_and_preserves_database_errors() {
    let (db, _tmp) = super::temp_db().await;
    let fetched_thread = db
        .read(|sql| {
            let value = sql.query_row("SELECT 42", [], |row| row.get::<_, i64>(0))?;
            Ok((value, std::thread::current().id()))
        })
        .process(|(value, reader)| {
            assert_eq!(value, 42);
            assert_ne!(reader, std::thread::current().id());
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
    assert!(matches!(
        error,
        DbError::Sqlite(coven::rusqlite::Error::QueryReturnedNoRows)
    ));

    let error = db
        .read(|_| Ok(()))
        .process(|()| Err::<(), _>(DbError::Sqlite(coven::rusqlite::Error::QueryReturnedNoRows)))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DbError::Sqlite(coven::rusqlite::Error::QueryReturnedNoRows)
    ));
}
