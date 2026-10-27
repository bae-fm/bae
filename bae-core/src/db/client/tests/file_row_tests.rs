//! The pending and abandoned lists name each file's row by index, whatever
//! the size of the library.

use super::super::abandoned::abandoned_rows_query;
use super::super::pending::pending_rows_query;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(db: &super::super::Database, statement: String) -> Vec<String> {
    db.read(move |sql| {
        sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| {
            row.get::<_, String>(3)
        })
        .map_err(super::super::DbError::from)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn the_pending_and_abandoned_lists_find_each_row_by_index() {
    let (db, _tmp) = super::temp_db().await;
    for statement in [pending_rows_query(), abandoned_rows_query()] {
        let plan = plan(&db, statement.clone()).await;
        for alias in [
            "named_file",
            "named_file_release",
            "named_file_album",
            "cover_release",
            "cover_album",
        ] {
            let steps: Vec<&String> = plan
                .iter()
                .filter(|step| step.split(' ').nth(1) == Some(alias))
                .collect();
            assert!(
                !steps.is_empty() && steps.iter().all(|step| step.starts_with("SEARCH")),
                "{alias} is not searched by index in {plan:?}"
            );
        }
    }
}
