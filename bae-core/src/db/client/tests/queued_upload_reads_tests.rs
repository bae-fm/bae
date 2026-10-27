//! The reads of what the upload queue holds of a release find its queued
//! files by index, one release's files at a time, not by reading the queue.

use super::super::queued_uploads::{
    RELEASE_FILE_QUEUED, RELEASE_HAS_QUEUED_FILE, RELEASE_QUEUED_FILE_COUNT,
};

async fn plan(db: &super::super::Database, statement: &'static str) -> Vec<String> {
    db.read(move |sql| {
        sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), ["id"], |row| {
            row.get::<_, String>(3)
        })
        .map_err(super::super::DbError::from)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_releases_queued_files_are_found_by_its_files() {
    let (db, _tmp) = super::temp_db().await;
    for statement in [
        RELEASE_QUEUED_FILE_COUNT,
        RELEASE_HAS_QUEUED_FILE,
        RELEASE_FILE_QUEUED,
    ] {
        let plan = plan(&db, statement).await;
        assert!(
            plan.iter().any(|step| step.starts_with("SEARCH")
                && step.contains("(table_name=? AND key=?)")),
            "the queue is not searched by row: {plan:?}"
        );
        assert!(
            !plan
                .iter()
                .any(|step| step.starts_with("SCAN f") || step.starts_with("SCAN upload")),
            "{statement} reads a whole table: {plan:?}"
        );
    }
}
