//! The playlist reads follow their indexes: no statement sorts after
//! reading.

use super::*;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(store: &Store, statement: String) -> Vec<String> {
    store
        .read(move |sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {statement}"),
                rusqlite::params_from_iter(std::iter::repeat_n(
                    "id",
                    statement.matches('?').count(),
                )),
                |row| row.get::<_, String>(3),
            )
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_entry_reads_follow_their_index_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    for statement in [playlist_entries_sql()] {
        let plan = plan(&db, statement.clone()).await;
        assert!(
            !plan
                .iter()
                .any(|step| step.starts_with("USE TEMP B-TREE FOR") && step.ends_with("ORDER BY")),
            "{statement} sorts after reading: {plan:?}"
        );
    }
}
