//! Playback's tables: what their writes keep, what their reads show and how
//! they are planned, over a store seeded with db's fixtures.

use super::*;
pub(crate) use crate::db::client::tests::*;

mod playback_now_tests;
mod playback_queue_additions_tests;
mod playback_queue_bench;
mod playback_queue_tests;
mod playback_timeline_tests;
mod queue_cover_tests;
pub(crate) mod window_cost;

/// The queue rows of `track_ids` queued in Up Next, as the queue value shows
/// them.
pub(crate) async fn queue_items(
    db: &Database,
    track_ids: &[&str],
) -> Vec<crate::playback::QueueItem> {
    PlaybackQueueTables::new(db.clone())
        .add_manual(track_ids.iter().map(|id| id.to_string()).collect(), ManualPlace::End)
        .await
        .unwrap();
    queue_value(db).await.unwrap().manual
}

#[tokio::test]
async fn queue_items_order_track_artist_names_inside_the_aggregate() {
    let (db, _tmp) = crate::db::client::tests::aggregate_ordering_tests::queue_db().await;
    let items = queue_items(&db, &[TRACK_A]).await;

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].display.artist_names, "Track Artist Name First, Track Artist Name Second");
}

/// Up Next longer than one SQL statement's parameter limit takes every track,
/// and the queue value shows each with what the track → release → album join
/// resolved.
#[tokio::test]
async fn a_queue_longer_than_one_in_clause_reads_every_track() {
    let (db, _tmp, track_ids) =
        crate::db::client::tests::in_clause_chunking_tests::chunked_track_db().await;
    let mut requested = track_ids.clone();
    requested.insert(SQL_MAX_IN_VARS / 2, bae_util::test_ids::test_uuid("missing-track"));

    let added =
        PlaybackQueueTables::new(db.clone()).add_manual(requested, ManualPlace::End).await.unwrap();
    assert_eq!(added, track_ids.len(), "the track the library lacks is left out");

    let items = queue_value(&db).await.unwrap().manual;
    let resolved: Vec<&str> = items.iter().map(|item| item.track_id.as_str()).collect();
    let expected: Vec<&str> = track_ids.iter().map(String::as_str).collect();
    assert_eq!(resolved, expected);
    assert!(items.iter().all(|item| item.display.album_title == "Album Title A"));
}
