//! The reads of what the upload queue holds of a release find its queued
//! files by index, one release's files at a time, not by reading the queue.

use crate::records::queued_uploads::{
    RELEASE_FILE_QUEUED, RELEASE_HAS_QUEUED_FILE, RELEASE_QUEUED_FILE_COUNT,
};
use bae_store::Store;

async fn plan(store: &Store, statement: &'static str) -> Vec<String> {
    store
        .read(move |sql| {
            sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), ["id"], |row| {
                row.get::<_, String>(3)
            })
            .map_err(crate::records::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_releases_queued_files_are_found_by_its_files() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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

/// A stored count below zero is no value a writer produces: reading
/// one fails naming its column instead of panicking where it is used.
#[test]
fn a_negative_upload_count_or_size_is_an_unreadable_column() {
    use crate::records::queued_uploads::release_upload_in_row;
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
}

/// The progress of a release's upload from its counts shows what the full
/// read's counts show.
#[test]
fn the_progress_of_a_release_shows_the_full_reads_counts() {
    use crate::records::queued_uploads::{release_upload_in_row, upload_progress};
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let counts = "SELECT 3 AS queued_count, 2 AS waiting_files, 1 AS sending_files, \
                  4 AS stored_files";
    let full = conn.query_row(counts, [], release_upload_in_row).unwrap().unwrap();
    let progress = upload_progress(2, 1).unwrap();
    assert_eq!(progress, crate::UploadProgress::of_release(&full));
    assert_eq!(progress, crate::UploadProgress { queued: 2, uploading: 1 });
}

/// Every release has a storage row, so when a release's upload read finds no
/// queued count, the library is broken, and the read fails instead of
/// reporting that nothing is queued.
#[test]
fn a_release_upload_read_without_a_storage_row_fails() {
    use crate::records::queued_uploads::release_upload_in_row;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let missing = "SELECT NULL AS queued_count, NULL AS waiting_files, 0 AS sending_files, \
                   NULL AS stored_files";
    assert!(matches!(
        conn.query_row(missing, [], release_upload_in_row),
        Err(rusqlite::Error::InvalidColumnType(..))
    ));
}
