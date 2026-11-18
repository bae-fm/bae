//! The status query finds the release file a failed state's record is about
//! by index, whatever the size of the library.

use crate::sync_status::status_query::status_query;
use bae_store::Store;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(store: &Store, statement: String) -> Vec<String> {
    store
        .read(move |sql| {
            sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| row.get::<_, String>(3))
                .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_cause_file_is_found_by_index() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let plan = plan(&db, status_query()).await;
    for alias in ["named", "named_file", "original_release_file", "original_file"] {
        let steps: Vec<&String> =
            plan.iter().filter(|step| step.split(' ').nth(1) == Some(alias)).collect();
        assert!(
            !steps.is_empty() && steps.iter().all(|step| step.starts_with("SEARCH")),
            "{alias} is not searched by index in {plan:?}"
        );
    }
}
