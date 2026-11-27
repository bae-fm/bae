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

/// The progress of a release's upload from its counts shows what the full
/// read's counts show.
#[test]
fn the_progress_of_a_release_shows_the_full_reads_counts() {
    use crate::records::queued_uploads::upload_progress;
    let full = crate::DbReleaseUpload { waiting: 2, sending: 1, stored: 4, missing: None };
    let progress = upload_progress(2, 1).unwrap();
    assert_eq!(progress, crate::UploadProgress::of_release(&full));
    assert_eq!(progress, crate::UploadProgress { queued: 2, uploading: 1 });
}
