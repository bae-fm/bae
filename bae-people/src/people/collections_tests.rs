//! The personal lists: a playlist's entries come back in the order their
//! statement states, read along an index rather than sorted after the read,
//! and playlists and profiles list by name in the language they are asked in.

use crate::people::collections::{ALBUM_RATINGS_SQL, PLAYLIST_ENTRIES_SQL, TRACK_STARS_SQL};
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
    let plan = plan(&db, PLAYLIST_ENTRIES_SQL.to_string()).await;
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "{PLAYLIST_ENTRIES_SQL} sorts after reading: {plan:?}"
    );
}

/// The album's ratings are found by the album's index, and one person's
/// stars for a release's tracks by the ratings' key, neither by a scan of
/// every rating.
#[tokio::test]
async fn ratings_are_found_by_index() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let album = plan(&db, ALBUM_RATINGS_SQL.to_string()).await;
    assert!(
        album.iter().any(|step| step.contains("ratings_album")),
        "the album's ratings are not read along ratings_album: {album:?}"
    );
    let tracks = plan(&db, TRACK_STARS_SQL.to_string()).await;
    assert!(
        !tracks.iter().any(|step| step.starts_with("SCAN")),
        "a person's track stars scan a whole table: {tracks:?}"
    );
}
