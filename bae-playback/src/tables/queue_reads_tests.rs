use super::*;
use crate::tables::tests::*;
use bae_store::query::SQL_MAX_IN_VARS;

#[tokio::test]
async fn queue_items_order_track_artist_names_inside_the_aggregate() {
    let (db, _tmp) =
        bae_records::test_fixtures::aggregate_ordering::queue_db(crate::TEST_SCHEMA).await;
    let items = queue_items(&db, &[TRACK_A]).await;

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].display.artist_line.plain_text(),
        "Track Artist Name First, Track Artist Name Second"
    );
}

/// Up Next longer than one SQL statement's parameter limit takes every track,
/// and the queue value shows each with what the track → release → album join
/// resolved.
#[tokio::test]
async fn a_queue_longer_than_one_in_clause_reads_every_track() {
    let (db, _tmp, track_ids) =
        bae_records::test_fixtures::chunked_tracks::chunked_track_db(crate::TEST_SCHEMA).await;
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
