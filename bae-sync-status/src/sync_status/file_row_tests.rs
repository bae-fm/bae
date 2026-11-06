//! The pending list names each file's row by index, whatever the size of the
//! library.

use crate::sync_status::pending::pending_rows_query;
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
async fn the_pending_list_finds_each_row_by_index() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let plan = plan(&db, pending_rows_query()).await;
    for alias in [
        "named",
        "named_file",
        "named_file_release",
        "named_file_album",
        "named_cover",
        "cover_release",
        "cover_album",
    ] {
        let steps: Vec<&String> =
            plan.iter().filter(|step| step.split(' ').nth(1) == Some(alias)).collect();
        assert!(
            !steps.is_empty() && steps.iter().all(|step| step.starts_with("SEARCH")),
            "{alias} is not searched by index in {plan:?}"
        );
    }
}
