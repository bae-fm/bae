//! What every table's tests share: db's fixtures, a library of one release,
//! and the queue rows of tracks queued in Up Next.

use super::*;
pub(super) use bae_records::test_fixtures::*;
pub(super) use bae_store::test_fixtures::*;

/// The queue rows of `track_ids` queued in Up Next, as the queue value shows
/// them.
pub(super) async fn queue_items(db: &Store, track_ids: &[&str]) -> Vec<crate::QueueItem> {
    PlaybackQueueTables::new(db.clone())
        .add_manual(track_ids.iter().map(|id| id.to_string()).collect(), ManualPlace::End)
        .await
        .unwrap();
    queue_value(db).await.unwrap().manual
}

/// `RELEASE_A` holding `count` tracks, in order, and their ids.
pub(super) async fn release_db(count: usize) -> (Store, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = test_records(&db);
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(RELEASE_A))
        .await
        .unwrap();
    let mut track_ids = Vec::new();
    for index in 0..count {
        let track_id = bae_util::test_ids::test_uuid(&format!("queue-track-{index}"));
        records.insert_track(&one_second_track(RELEASE_A, &track_id, index)).await.unwrap();
        track_ids.push(track_id);
    }
    (db, tmp, track_ids)
}

/// `count` tracks, each the one track of its own release on `ALBUM_A`, in
/// order, and their ids: a test deletes one track by deleting its release.
pub(super) async fn single_track_releases_db(
    count: usize,
) -> (Store, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = test_records(&db);
    let mut track_ids = Vec::new();
    for index in 0..count {
        let release_id = bae_util::test_ids::test_uuid(&format!("queue-release-{index}"));
        let track_id = bae_util::test_ids::test_uuid(&format!("queue-track-{index}"));
        records
            .insert_release_with_tracks(
                &bae_records::DbRelease::new_test(ALBUM_A, &release_id),
                &[bae_records::DbReleaseMedium::new_test(&release_id)],
                &[one_second_track(&release_id, &track_id, index)],
                &[],
            )
            .await
            .unwrap();
        track_ids.push(track_id);
    }
    (db, tmp, track_ids)
}

/// Track `index` of `release_id`, one second long, titled by its index.
fn one_second_track(release_id: &str, track_id: &str, index: usize) -> bae_records::DbTrack {
    let title = format!("Track Title {index}");
    let number = u32::try_from(index + 1).unwrap();
    bae_records::DbTrack {
        duration_ms: 1000,
        ..bae_records::DbTrack::new_test(release_id, track_id, &title, number)
    }
}

/// Delete the release `track_id` is on, and with it the track.
pub(super) async fn delete_release_of(db: &Store, track_id: &str) {
    let records = test_records(db);
    let track = records.find_track_by_id(track_id).await.unwrap().expect("the track");
    delete_release(&records, &track.release_id).await;
}
