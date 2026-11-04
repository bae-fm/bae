//! The personal lists come back in the order their statements state, read
//! along an index rather than sorted after the read.

use crate::client::collections::{PLAYLISTS_SQL, PLAYLIST_ENTRIES_SQL};
use crate::client::people::PROFILES_SQL;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(db: &crate::client::Database, statement: &'static str) -> Vec<String> {
    db.read(move |sql| {
        sql.query(
            &format!("EXPLAIN QUERY PLAN {statement}"),
            rusqlite::params_from_iter(std::iter::repeat_n("id", statement.matches('?').count())),
            |row| row.get::<_, String>(3),
        )
        .map_err(crate::client::DbError::from)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn personal_lists_read_in_order_without_a_sort() {
    let (db, _temp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    for statement in [PLAYLISTS_SQL, PLAYLIST_ENTRIES_SQL, PROFILES_SQL] {
        let plan = plan(&db, statement).await;
        assert!(
            !plan.iter().any(|step| step.contains("TEMP B-TREE")),
            "{statement} sorts after reading: {plan:?}"
        );
    }
}
