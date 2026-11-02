//! The queue row's count of additions to Up Next: written by the write that
//! adds the tracks, read by the screens confirming each addition.

use super::playback_queue_tests::release_db;
use crate::db::DbError;
use crate::db::ManualPlace;
use crate::queue::QueueAddition;

/// Each addition that puts tracks in counts one, with how many it put in,
/// wherever in Up Next they go; two alike in a row are two values. Tracks the
/// library does not hold go in uncounted, and an addition of none of its
/// tracks counts nothing.
#[tokio::test]
async fn each_addition_counts_once_with_its_tracks() {
    let (db, _tmp, tracks) = release_db(3).await;
    let queue = crate::db::PlaybackQueueTables::new(db.clone());
    assert_eq!(db.queue_addition().await.unwrap(), None);

    queue
        .add_manual(
            vec![tracks[0].clone(), "not-a-track".to_string(), tracks[1].clone()],
            ManualPlace::End,
        )
        .await
        .unwrap();
    assert_eq!(db.queue_addition().await.unwrap(), Some(QueueAddition { number: 1, tracks: 2 }));

    queue.add_manual(vec!["not-a-track".to_string()], ManualPlace::Front).await.unwrap();
    assert_eq!(
        db.queue_addition().await.unwrap().map(|addition| addition.number),
        Some(1),
        "nothing went in"
    );

    for (number, place) in [(2, ManualPlace::Front), (3, ManualPlace::At(1))] {
        queue.add_manual(vec![tracks[2].clone(), tracks[0].clone()], place).await.unwrap();
        assert_eq!(db.queue_addition().await.unwrap(), Some(QueueAddition { number, tracks: 2 }));
    }
}

/// Starting playback with the library forgets the additions, whether it
/// resumes the queue or empties it, so none confirms again.
#[tokio::test]
async fn starting_playback_forgets_the_additions() {
    let (db, _tmp, tracks) = release_db(1).await;
    let queue = crate::db::PlaybackQueueTables::new(db.clone());
    for resume in [true, false] {
        queue.add_manual(vec![tracks[0].clone()], ManualPlace::End).await.unwrap();
        assert!(db.queue_addition().await.unwrap().is_some());

        db.settle_playback_queue(resume).await.unwrap();
        assert_eq!(db.queue_addition().await.unwrap(), None, "resume: {resume}");
    }
}

/// The count and the number of the addition it belongs to are set together.
#[tokio::test]
async fn the_schema_refuses_a_count_without_an_addition() {
    let (db, _tmp, _tracks) = release_db(1).await;
    for sql in [
        "UPDATE playback_queue SET added_tracks = 2",
        "UPDATE playback_queue SET additions = 1",
        "UPDATE playback_queue SET additions = 1, added_tracks = 0",
    ] {
        let statement = sql.to_string();
        let refused = db
            .call(move |conn| conn.execute_batch(&statement).map_err(DbError::from))
            .await
            .is_err();
        assert!(refused, "{sql}");
    }
}

/// The live query delivers one value per addition and none for writes to
/// the queue that add nothing.
#[tokio::test]
async fn the_live_query_delivers_each_addition() {
    let (db, _tmp, tracks) = release_db(2).await;
    let queue = crate::db::PlaybackQueueTables::new(db.clone());
    let mut additions = db.subscribe_queue_addition();
    assert_eq!(next(&mut additions).await, None);

    queue.set_repeat(crate::playback::RepeatMode::Track).await.unwrap();
    queue.add_manual(vec![tracks[1].clone()], ManualPlace::End).await.unwrap();
    assert_eq!(
        next(&mut additions).await,
        Some(QueueAddition { number: 1, tracks: 1 }),
        "the repeat change delivered nothing of its own"
    );
}

/// The query's next value, within five seconds.
async fn next(additions: &mut coven::LiveQuery<Option<QueueAddition>>) -> Option<QueueAddition> {
    tokio::time::timeout(std::time::Duration::from_secs(5), additions.next())
        .await
        .expect("a value in time")
        .expect("the query runs")
        .expect("the read succeeds")
}
