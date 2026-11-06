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
            Ok::<_, DbError>(reader)
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

/// A write whose closure panics fails with the bug, and the store, which
/// awaits the call, reports it to its sink once; the failure holds it as
/// reported, so no caller above reports it again.
#[tokio::test]
async fn a_call_that_panics_is_reported_once_by_the_store() {
    use bae_util::task_failure::ReportedBug;
    let tmp = tempfile::TempDir::new().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(tmp.path()).unwrap().coven_app_dir());
    let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
    let handle = crate::configure_store(installation.builder(), Schema::new(&[]))
        .clock(clock.clone())
        .create_store("Test Library")
        .await
        .unwrap();
    let heard = coven::task::HeardBugs::shared();
    let db = crate::Store::new(handle, clock, heard.clone());

    let error =
        db.call(|_| -> Result<(), DbError> { panic!("note 43 has no title") }).await.unwrap_err();

    let reported = heard.take();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].message, "note 43 has no title");
    assert_eq!(error.reported_bug(), Some(&reported[0]));
    db.close().await.unwrap();
}

/// A closure's own failure comes back from a read and from a write as the
/// error it was, not as a coven error that wraps it.
#[tokio::test]
async fn a_closure_failure_comes_back_as_itself() {
    let (db, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;

    let read = db.read(|_| Err::<(), _>(DbError::unreadable("x", 1))).await.unwrap_err();
    assert!(matches!(read, DbError::Unreadable { .. }), "{read:?}");

    let write = db.call(|_| Err::<(), _>(DbError::unreadable("x", 1))).await.unwrap_err();
    assert!(matches!(write, DbError::Unreadable { .. }), "{write:?}");
}
