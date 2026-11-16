use super::*;
use crate::tables::tests::*;
use crate::tables::{queue_upcoming, ContextFill, ManualPlace, PlaybackQueueTables, ReleaseStart};
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

/// Play `RELEASE_A`'s `tracks` from the second, shuffled when `shuffled`, and
/// queue the last two by hand.
async fn up_next_with_both_lanes(db: &Store, tracks: &[String], shuffled: bool) {
    let queue = PlaybackQueueTables::new(db.clone());
    queue.set_shuffle(shuffled).await.unwrap();
    queue
        .fill(ContextFill::Release {
            release_id: RELEASE_A.to_string(),
            start: ReleaseStart::Track(tracks[1].clone()),
        })
        .await
        .unwrap();
    queue.add_manual(tracks[tracks.len() - 2..].to_vec(), ManualPlace::End).await.unwrap();
}

async fn session_up_next(db: &Store, limit: u64) -> Vec<String> {
    let items = db.read(move |sql| session_up_next_on(&sql, limit)).await.unwrap();
    items.into_iter().map(|item| item.track_id).collect()
}

/// The session's list is Up Next, then the context lane's tracks after the
/// playing one in the order the lane reads in, cut at the limit, shuffled or
/// not.
#[tokio::test]
async fn the_session_lists_up_next_then_the_context_up_to_its_limit() {
    for shuffled in [false, true] {
        let (db, _tmp, tracks) = release_db(6).await;
        up_next_with_both_lanes(&db, &tracks, shuffled).await;
        let manual = queue_value(&db).await.unwrap().manual;
        let context = queue_upcoming(&db).await.unwrap();
        let every: Vec<String> =
            manual.iter().chain(&context).map(|item| item.track_id.clone()).collect();
        assert_eq!(manual.len(), 2);
        assert!(context.len() >= 4, "the tracks after the playing one: {context:?}");

        assert_eq!(session_up_next(&db, 100).await, every, "shuffled: {shuffled}");
        assert_eq!(session_up_next(&db, 3).await, every[..3], "shuffled: {shuffled}");
        assert_eq!(session_up_next(&db, 1).await, every[..1], "shuffled: {shuffled}");
        assert!(session_up_next(&db, 0).await.is_empty());
    }
}
