//! The playlists' plain reads: the listing of every playlist with its count,
//! length and cover, and the detail of a playlist of 50, 500 and 10,000
//! entries.

use super::seed::{sizes, SEEDS};
use bae_store::test_fixtures::screen_cost::{plain_cost, size_name};
use bae_store::Store;
use rusqlite::types::Value;

/// How many releases a cover grid shows.
const COVER_GRID: i64 = 4;

/// The listing: each playlist with its entries counted, summed in length and
/// the first releases its entries name.
const LISTING: &str = "SELECT pl.id, pl.name, pl.person_id, pl.audience, \
        (SELECT COUNT(*) FROM playlist_entries e WHERE e.playlist_id = pl.id) AS track_count, \
        (SELECT COALESCE(SUM(t.duration_ms), 0) FROM playlist_entries e \
            JOIN tracks t ON t.id = e.track_id WHERE e.playlist_id = pl.id) AS total_duration_ms, \
        (SELECT json_group_array(json_object('release_id', release_id)) \
         FROM (SELECT t.release_id, MIN(e.position) AS first \
               FROM playlist_entries e JOIN tracks t ON t.id = e.track_id \
               WHERE e.playlist_id = pl.id GROUP BY t.release_id \
               ORDER BY first LIMIT ?1)) AS cover_releases, \
        COUNT(*) OVER () AS total \
    FROM playlists pl ORDER BY pl.name COLLATE natural_order, pl.id";

/// A playlist's entries with what each row shows, in order.
const ENTRIES: &str = "SELECT e.position_id, e.position, t.id, t.title, t.duration_ms, \
        a.title AS album_title, r.id AS release_id, \
        COUNT(*) OVER () AS track_count, SUM(t.duration_ms) OVER () AS total_duration_ms \
    FROM playlist_entries e \
    JOIN tracks t ON t.id = e.track_id \
    JOIN releases r ON r.id = t.release_id \
    JOIN albums a ON a.id = r.album_id \
    WHERE e.playlist_id = ?1 ORDER BY e.position, e.position_id";

/// The id of the playlist holding the number of entries nearest `target`.
async fn playlist_of(db: &Store, target: i64) -> String {
    db.read(move |read| {
        read.query_row(
            "SELECT playlist_id FROM playlist_entries GROUP BY playlist_id \
             ORDER BY ABS(COUNT(*) - ?1), playlist_id LIMIT 1",
            [target],
            |row| row.get(0),
        )
        .map_err(Into::into)
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn playlists_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        let at = size_name(size);
        let rows = plain_cost(
            &db,
            &format!("playlists/listing@{at}"),
            Some(50.0),
            LISTING,
            vec![Value::Integer(COVER_GRID)],
        )
        .await;
        assert_eq!(rows, 201);
        for entries in [50, 500, 10_000] {
            let playlist = playlist_of(&db, entries).await;
            let rows = plain_cost(
                &db,
                &format!("playlists/detail-{entries}@{at}"),
                Some(50.0),
                ENTRIES,
                vec![Value::Text(playlist)],
            )
            .await;
            assert!(rows >= 10, "the playlist of about {entries} entries has them");
        }
    }
}
