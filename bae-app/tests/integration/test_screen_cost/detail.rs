//! The detail pages' plain reads: an album's release as its tracks with their
//! credits and works, its credits and its files with their totals, for a
//! 12-track release, a release of an album with two, and the 120- and
//! 600-track box sets; and library search by keystroke.

use super::seed::{sizes, SEEDS};
use bae_records::test_fixtures::shaped_library;
use bae_store::test_fixtures::screen_cost::{plain_cost, size_name};
use bae_store::Store;
use rusqlite::types::Value;

/// A release's tracks in playing order, each with the credits printed on it
/// and the works it performs nested as JSON.
const TRACKS: &str =
    "SELECT t.id, t.title, t.duration_ms, t.side, t.track_number, m.position AS disc, \
        (SELECT json_group_array(json_object('role', c.role, 'artist', credited.name)) \
         FROM track_artist_roles c JOIN artists credited ON credited.id = c.artist_id \
         WHERE c.track_id = t.id) AS credits, \
        (SELECT json_group_array(json_object('work', w.id, 'title', w.title)) \
         FROM track_works tw JOIN works w ON w.id = tw.work_id \
         WHERE tw.track_id = t.id) AS works, \
        SUM(t.duration_ms) OVER () AS total_duration_ms \
    FROM tracks t JOIN release_mediums m ON m.id = t.medium_id \
    WHERE t.release_id = ?1 ORDER BY t.position, t.id";

/// A release's credits: its own, then those of its tracks, each with the
/// artist's name.
const CREDITS: &str = "SELECT c.role, c.role_words, credited.name, NULL AS track_id \
        FROM release_artist_roles c JOIN artists credited ON credited.id = c.artist_id \
        WHERE c.release_id = ?1 \
    UNION ALL \
    SELECT c.role, c.role_words, credited.name, c.track_id \
        FROM track_artist_roles c JOIN artists credited ON credited.id = c.artist_id \
        JOIN tracks t ON t.id = c.track_id WHERE t.release_id = ?1";

/// A release's files with how many there are and their size, as coven
/// records it.
const FILES: &str = "SELECT f.id, f.original_filename, f.content_type, \
        f.content_type LIKE 'image/%' AS is_image, coven.size, \
        COUNT(*) OVER () AS file_count, SUM(coven.size) OVER () AS total_size \
    FROM release_files f JOIN coven_files coven ON coven.id = f.file \
    WHERE f.release_id = ?1 ORDER BY f.original_filename COLLATE natural_order, f.id";

async fn release_page(db: &Store, size: u64) {
    let at = size_name(size);
    let two_releases: String = db
        .read(|read| {
            read.query_row(
                "SELECT r.id FROM releases r WHERE r.album_id IN \
                 (SELECT album_id FROM releases GROUP BY album_id HAVING COUNT(*) = 2) \
                 ORDER BY r.id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    for (name, release) in [
        ("12-tracks", shaped_library::release_id(1, 0)),
        ("2-releases", two_releases),
        ("120-tracks", shaped_library::release_id(25, 0)),
        ("600-tracks", shaped_library::release_id(250, 0)),
    ] {
        for (part, sql, bar) in
            [("tracks", TRACKS, 50.0), ("credits", CREDITS, 50.0), ("files", FILES, 50.0)]
        {
            let rows = plain_cost(
                db,
                &format!("album-page/{name}/{part}@{at}"),
                Some(bar),
                sql,
                vec![Value::Text(release.clone())],
            )
            .await;
            if part == "tracks" {
                assert!(rows >= 12, "{name} has its tracks");
            }
        }
    }
}

/// Library search by keystroke: for each kind the first matches and the
/// total, the text grown a keystroke at a time. The seed's names are
/// "Album 12345", "Artist 123456", "Work 000123" and "Track 000123-0-004".
async fn search(db: &Store, size: u64) {
    let at = size_name(size);
    for (kind, table, column, keystrokes) in [
        ("albums", "albums", "title", ["al", "alb", "album", "album 1"]),
        ("artists", "artists", "name", ["ar", "art", "artist", "artist 0"]),
        ("works", "works", "title", ["wo", "wor", "work", "work 0"]),
        ("tracks", "tracks", "title", ["tr", "tra", "track", "track 0"]),
    ] {
        for text in keystrokes {
            let pattern = Value::Text(text.to_string());
            let metric = format!("search/{kind}/{}@{at}", text.replace(' ', "_"));
            plain_cost(
                db,
                &format!("{metric}/window"),
                Some(50.0),
                &format!(
                    "SELECT id, {column} FROM {table} \
                     WHERE bae_fold_case({column}) GLOB bae_search_pattern(?1) \
                     ORDER BY {column} COLLATE natural_order, id LIMIT 20"
                ),
                vec![pattern.clone()],
            )
            .await;
            plain_cost(
                db,
                &format!("{metric}/count"),
                Some(50.0),
                &format!(
                    "SELECT COUNT(*) FROM {table} \
                     WHERE bae_fold_case({column}) GLOB bae_search_pattern(?1)"
                ),
                vec![pattern],
            )
            .await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn detail_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        release_page(&db, size).await;
        search(&db, size).await;
    }
}
