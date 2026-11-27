//! The personal lists: a playlist's entries come back in the order their
//! statement states, read along an index rather than sorted after the read,
//! and playlists and profiles list by name in the language they are asked in.

use crate::people::collections::album_ratings_sql;
use crate::people::collections::EntriesOfPlaylist;
use bae_store::Store;

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
async fn a_playlists_entries_read_in_order_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let plan = db
        .read(|sql| {
            Ok(sql.query(
                &format!("EXPLAIN QUERY PLAN {}", EntriesOfPlaylist::SQL),
                rusqlite::named_params! { ":playlist": "id" },
                |row| row.get::<_, String>(3),
            )?)
        })
        .await
        .unwrap();
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the entries sort after reading: {plan:?}"
    );
}

/// SQLite finds the album's ratings through the album's index, without
/// scanning every rating.
#[tokio::test]
async fn ratings_are_found_by_index() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let album = plan(&db, album_ratings_sql()).await;
    assert!(
        album.iter().any(|step| step.contains("ratings_album")),
        "the album's ratings are not read along ratings_album: {album:?}"
    );
}
