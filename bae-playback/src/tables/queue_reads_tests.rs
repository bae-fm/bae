//! The queue row's count of additions to Up Next: written by the write that
//! adds the tracks, read by the screens confirming each addition.

use super::*;
use crate::tables::tests::*;
use crate::QueueAddition;
use bae_store::query::SQL_MAX_IN_VARS;
use bae_util::test_channels::HANG_GUARD;

/// Each addition that puts tracks in counts one, with how many it put in,
/// wherever in Up Next they go; two alike in a row are two values. Tracks the
/// library does not hold go in uncounted, and an addition of none of its
/// tracks counts nothing.
#[tokio::test]
async fn each_addition_counts_once_with_its_tracks() {
    let (db, _tmp, tracks) = release_db(3).await;
    let queue = PlaybackQueueTables::new(db.clone());
    assert_eq!(queue_addition(&db).await.unwrap(), None);

    queue
        .add_manual(
            vec![tracks[0].clone(), "not-a-track".to_string(), tracks[1].clone()],
            ManualPlace::End,
        )
        .await
        .unwrap();
    assert_eq!(queue_addition(&db).await.unwrap(), Some(QueueAddition { number: 1, tracks: 2 }));

    queue.add_manual(vec!["not-a-track".to_string()], ManualPlace::Front).await.unwrap();
    assert_eq!(
        queue_addition(&db).await.unwrap().map(|addition| addition.number),
        Some(1),
        "nothing went in"
    );

    for (number, place) in [(2, ManualPlace::Front), (3, ManualPlace::At(1))] {
        queue.add_manual(vec![tracks[2].clone(), tracks[0].clone()], place).await.unwrap();
        assert_eq!(queue_addition(&db).await.unwrap(), Some(QueueAddition { number, tracks: 2 }));
    }
}

/// Starting playback with the library forgets the additions, whether it
/// resumes the queue or empties it, so none confirms again.
#[tokio::test]
async fn starting_playback_forgets_the_additions() {
    let (db, _tmp, tracks) = release_db(1).await;
    let queue = PlaybackQueueTables::new(db.clone());
    for resume in [true, false] {
        queue.add_manual(vec![tracks[0].clone()], ManualPlace::End).await.unwrap();
        assert!(queue_addition(&db).await.unwrap().is_some());

        settle_playback_queue(&db, resume).await.unwrap();
        assert_eq!(queue_addition(&db).await.unwrap(), None, "resume: {resume}");
    }
}

/// The live query delivers one value per addition and none for writes to
/// the queue that add nothing.
#[tokio::test]
async fn the_live_query_delivers_each_addition() {
    let (db, _tmp, tracks) = release_db(2).await;
    let queue = PlaybackQueueTables::new(db.clone());
    let mut additions = subscribe_queue_addition(&db);
    assert_eq!(next(&mut additions).await, None);

    queue.set_repeat(crate::RepeatMode::Track).await.unwrap();
    queue.add_manual(vec![tracks[1].clone()], ManualPlace::End).await.unwrap();
    assert_eq!(
        next(&mut additions).await,
        Some(QueueAddition { number: 1, tracks: 1 }),
        "the repeat change delivered nothing of its own"
    );
}

/// The query's next value.
async fn next(
    additions: &mut coven::LiveQuery<Option<QueueAddition>, bae_store::DbError>,
) -> Option<QueueAddition> {
    tokio::time::timeout(HANG_GUARD, additions.next())
        .await
        .expect("a value in time")
        .expect("the query runs")
        .expect("the read succeeds")
}

#[tokio::test]
async fn queue_items_order_track_artist_names_inside_the_aggregate() {
    let (db, _tmp) =
        bae_records::test_fixtures::aggregate_ordering::queue_db(crate::TEST_SCHEMA).await;
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
