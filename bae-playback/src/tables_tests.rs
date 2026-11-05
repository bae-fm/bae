//! What every table's tests share: db's fixtures, a library of one release,
//! and the queue rows of tracks queued in Up Next.

use super::*;
pub(super) use bae_db::test_fixtures::*;

/// The queue rows of `track_ids` queued in Up Next, as the queue value shows
/// them.
pub(super) async fn queue_items(db: &Database, track_ids: &[&str]) -> Vec<crate::QueueItem> {
    PlaybackQueueTables::new(db.clone())
        .add_manual(track_ids.iter().map(|id| id.to_string()).collect(), ManualPlace::End)
        .await
        .unwrap();
    queue_value(db).await.unwrap().manual
}

/// `RELEASE_A` holding `count` tracks, in order, and their ids.
pub(super) async fn release_db(count: usize) -> (Database, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let track_ids: Vec<String> = (0..count)
        .map(|index| bae_util::test_ids::test_uuid(&format!("queue-track-{index}")))
        .collect();
    let seed = track_ids.clone();
    db.call(move |conn| {
        seed_artist_album_release(conn)?;
        let medium_id = bae_db::identity::release_medium_id(RELEASE_A, 1);
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?1, ?2, 1, 'cd', 1, '2026-01-01T00:00:00Z')",
            params![medium_id, RELEASE_A],
        )?;
        for (index, track_id) in seed.iter().enumerate() {
            conn.execute(
                "INSERT INTO tracks \
                     (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
                 VALUES (?3, ?1, ?5, ?2, ?4, 1, ?3 + 1, 1000, '2026-01-01T00:00:00Z')",
                params![track_id, format!("Track Title {index}"), index as i64, medium_id, RELEASE_A],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp, track_ids)
}
