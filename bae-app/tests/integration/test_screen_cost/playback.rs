//! The playback screens' plain reads: now playing, the queue's head and its
//! upcoming window and count, each timed beside what the screen reads today
//! (`<metric>+today`, for reference).

use super::seed::{sizes, SEEDS};
use bae_store::test_fixtures::screen_cost::{
    call_cost, list_costs, plain_cost, size_name, ListShape,
};
use bae_store::Store;

/// The track a screen shows: its title, release, album and artist names.
const TRACK_DISPLAY: &str = "t.title AS track_title, a.title AS album_title, r.id AS release_id, \
    (SELECT GROUP_CONCAT(credited.name, ', ' ORDER BY credit.position) \
     FROM album_artists credit \
     JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
     JOIN artists credited ON credited.id = shown.shown_artist_id \
     WHERE credit.album_id = a.id) AS artists";

const TRACK_JOINS: &str =
    "JOIN releases r ON r.id = t.release_id JOIN albums a ON a.id = r.album_id";

/// What the playing track bar and now-playing page read: the state, the
/// track it names and the queue's repeat mode, as one statement.
async fn now_playing(db: &Store, size: u64) {
    let at = size_name(size);
    let sql = format!(
        "SELECT n.state, n.track_id, n.duration_ms, q.repeat, {TRACK_DISPLAY}, \
                t.id IS NULL AND n.track_id IS NOT NULL AS track_gone \
         FROM playback_now n CROSS JOIN playback_queue q \
         LEFT JOIN tracks t ON t.id = n.track_id \
         LEFT JOIN releases r ON r.id = t.release_id \
         LEFT JOIN albums a ON a.id = r.album_id"
    );
    let rows = plain_cost(db, &format!("playback/now-playing@{at}"), Some(2.0), &sql, vec![]).await;
    assert_eq!(rows, 1);
    call_cost(&format!("playback/now-playing+today@{at}"), 2.0, || {
        bae_playback::tables::now_playing(db)
    })
    .await;
}

/// The queue head: repeat and shuffle, the lane's source, whether anything
/// follows or precedes, and Up Next's tracks.
async fn queue_head(db: &Store, size: u64) {
    let at = size_name(size);
    let head = "SELECT q.repeat, q.shuffled, q.source_kind, q.source_release_id, \
        (SELECT a.title FROM releases r JOIN albums a ON a.id = r.album_id \
         WHERE r.id = q.source_release_id) AS source_title, \
        (EXISTS (SELECT 1 FROM playback_manual) \
         OR EXISTS (SELECT 1 FROM playback_context c WHERE c.ordered > q.ordered_cursor) \
         OR q.repeat <> 'off') AS has_next, \
        EXISTS (SELECT 1 FROM playback_context c WHERE c.ordered < q.ordered_cursor) AS has_previous \
        FROM playback_queue q";
    let rows = plain_cost(db, &format!("playback/queue-head@{at}"), Some(2.0), head, vec![]).await;
    assert_eq!(rows, 1);
    let manual = format!(
        "SELECT 'm' || m.entry, m.track_id, m.position, {TRACK_DISPLAY} \
         FROM playback_manual m JOIN tracks t ON t.id = m.track_id {TRACK_JOINS} \
         ORDER BY m.position"
    );
    let rows = plain_cost(db, &format!("playback/up-next@{at}"), Some(2.0), &manual, vec![]).await;
    assert!(rows > 0, "Up Next holds tracks");
    call_cost(&format!("playback/queue-head+today@{at}"), 2.0, || {
        bae_playback::tables::queue_value(db)
    })
    .await;
}

/// The upcoming window and count of the context lane.
async fn upcoming(db: &Store, size: u64) {
    list_costs(
        db,
        size,
        &ListShape {
            metric: "playback/upcoming",
            select: &format!("c.entry, c.track_id, c.ordered, {TRACK_DISPLAY}"),
            source: "FROM playback_context c JOIN tracks t ON t.id = c.track_id \
                     JOIN releases r ON r.id = t.release_id JOIN albums a ON a.id = r.album_id \
                     WHERE c.ordered > -1",
            key: "c.ordered",
            tie: "c.entry",
            descending: false,
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn playback_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        now_playing(&db, size).await;
        queue_head(&db, size).await;
        upcoming(&db, size).await;
    }
}
