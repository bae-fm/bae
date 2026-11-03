use super::super::*;
use super::{exec, exec_batch};
use crate::test_fixtures::live_query::*;
use std::collections::BTreeSet;
use std::time::Duration;

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db().await;
    exec(
        &db,
        "INSERT INTO albums
         (id, title, is_compilation, created_at) VALUES (?1, 'Other Album', 0, '2026-01-01T00:00:00Z')",
        &[OTHER_ALBUM_ID],
    )
    .await;
    // The request only gives the test a change of its own to make; the query
    // reads the same row whatever it is.
    let mut live = db.inner.handle.subscribe_reconfigurable(0_u8, |_, sql| {
        sql.query_row("SELECT title FROM albums WHERE id = ?1", params![ALBUM_ID], |row| {
            row.get::<_, String>(0)
        })
        .map_err(CovenError::from)
    });
    assert_eq!(live.next().await.expect("the query stays open").result.unwrap(), "Album Title");

    exec(&db, "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1", &[OTHER_ALBUM_ID])
        .await;

    let moved = next_run_is_only_the_request(&mut live, 1).await;
    assert_eq!(moved.result.unwrap(), "Album Title");
}

#[path = "live_query_tests/id_sets.rs"]
mod id_sets;
