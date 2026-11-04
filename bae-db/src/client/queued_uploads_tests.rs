//! The reads of what the upload queue holds of a release find its queued
//! files by index, one release's files at a time, not by reading the queue.

use crate::client::queued_uploads::{
    RELEASE_FILE_QUEUED, RELEASE_HAS_QUEUED_FILE, RELEASE_QUEUED_FILE_COUNT,
};

async fn plan(db: &crate::client::Database, statement: &'static str) -> Vec<String> {
    db.read(move |sql| {
        sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), ["id"], |row| row.get::<_, String>(3))
            .map_err(crate::client::DbError::from)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_releases_queued_files_are_found_by_its_files() {
    let (db, _tmp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    for statement in [RELEASE_QUEUED_FILE_COUNT, RELEASE_HAS_QUEUED_FILE, RELEASE_FILE_QUEUED] {
        let plan = plan(&db, statement).await;
        assert!(
            plan.iter().any(|step| step.starts_with("SEARCH") && step.contains("(file=?)")),
            "the queue is not searched by file: {plan:?}"
        );
        assert!(
            !plan.iter().any(|step| step.starts_with("SCAN f") || step.starts_with("SCAN upload")),
            "{statement} reads a whole table: {plan:?}"
        );
    }
}

/// A stored count or size below zero is no value a writer produces: reading
/// one fails naming its column instead of panicking where it is used.
#[test]
fn a_negative_upload_count_or_size_is_an_unreadable_column() {
    use crate::client::queued_uploads::{
        release_upload_in_row, sending_file_in_row, sending_release_in_row,
    };
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let failure = |error: rusqlite::Error| match error {
        rusqlite::Error::FromSqlConversionFailure(_, _, cause) => cause.to_string(),
        other => panic!("not a column-conversion error: {other}"),
    };

    let upload = conn
        .query_row(
            "SELECT 1 AS queued_count, -1 AS waiting_files, 2 AS sending_files, 0 AS stored_files",
            [],
            release_upload_in_row,
        )
        .unwrap_err();
    assert!(failure(upload).starts_with("waiting_files -1"));

    let file = conn
        .query_row(
            "SELECT '00000000-0000-0000-0000-000000000001' AS file, 'r' AS release_id, \
             -5 AS file_size",
            [],
            sending_file_in_row,
        )
        .unwrap_err();
    assert!(failure(file).starts_with("file_size -5"));

    let release = conn
        .query_row(
            "SELECT 'r' AS release_id, -400 AS bytes_stored, 1000 AS bytes_total",
            [],
            sending_release_in_row,
        )
        .unwrap_err();
    assert!(failure(release).starts_with("bytes_stored -400"));
}
