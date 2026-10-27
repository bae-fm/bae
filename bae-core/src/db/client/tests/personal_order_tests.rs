//! The personal lists come back in the order their statements state, read
//! along an index rather than sorted after the read.

use super::super::collections::{PLAYLISTS_SQL, PLAYLIST_ENTRIES_SQL};
use super::super::people::PROFILES_SQL;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(db: &super::super::Database, statement: &'static str) -> Vec<String> {
    db.read(move |sql| {
        sql.query(
            &format!("EXPLAIN QUERY PLAN {statement}"),
            rusqlite::params_from_iter(std::iter::repeat_n("id", statement.matches('?').count())),
            |row| row.get::<_, String>(3),
        )
        .map_err(super::super::DbError::from)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn personal_lists_read_in_order_without_a_sort() {
    let (db, _temp) = super::temp_db().await;
    for statement in [PLAYLISTS_SQL, PLAYLIST_ENTRIES_SQL, PROFILES_SQL] {
        let plan = plan(&db, statement).await;
        assert!(
            !plan.iter().any(|step| step.contains("TEMP B-TREE")),
            "{statement} sorts after reading: {plan:?}"
        );
    }
}
