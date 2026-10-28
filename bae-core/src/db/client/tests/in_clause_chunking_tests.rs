use super::super::*;

async fn chunked_track_db() -> (Database, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = super::temp_db().await;
    let track_count = SQL_MAX_IN_VARS * 45;
    let track_ids: Vec<String> = (0..track_count)
        .map(|index| bae_test_support::test_uuid(&format!("track-{index}")))
        .collect();
    let seed_track_ids = track_ids.clone();
    db.call(move |conn| {
        super::seed_artist_album_release(conn)?;
        // The ids are minted in Rust, not in SQL: coven takes only canonical
        // UUIDs on a synced row, and the test's assertions need the same
        // values the seed wrote.
        let medium_id =
            crate::db::identity::release_medium_id("0252dedb-ee39-4547-8803-438dbeb57a64", 1);
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?1, '0252dedb-ee39-4547-8803-438dbeb57a64', 1, 'cd', 1, '2026-01-01T00:00:00Z')",
            params![medium_id],
        )?;
        for (index, track_id) in seed_track_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO tracks \
                     (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
                 VALUES (?3, ?1, '0252dedb-ee39-4547-8803-438dbeb57a64', ?2, ?4, 1, ?3 + 1, 1000, '2026-01-01T00:00:00Z')",
                params![track_id, format!("Track Title {index}"), index as i64, medium_id],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp, track_ids)
}

#[tokio::test]
async fn filter_existing_track_ids_merges_chunks() {
    let (db, _tmp, track_ids) = chunked_track_db().await;
    let mut requested = track_ids.clone();
    requested.insert(SQL_MAX_IN_VARS / 2, "missing-track".to_string());

    let mut existing = db.filter_existing_track_ids(&requested).await.unwrap();
    existing.sort();
    let mut expected_existing = track_ids;
    expected_existing.sort();
    assert_eq!(existing, expected_existing);
}

/// Up Next longer than one SQL statement's parameter limit takes every track,
/// and the queue value shows each with what the track → release → album join
/// resolved.
#[tokio::test]
async fn a_queue_longer_than_one_in_clause_reads_every_track() {
    let (db, _tmp, track_ids) = chunked_track_db().await;
    let mut requested = track_ids.clone();
    requested.insert(
        SQL_MAX_IN_VARS / 2,
        bae_test_support::test_uuid("missing-track"),
    );

    let added = db
        .playback_queue_tables()
        .add_manual(requested, ManualPlace::End)
        .await
        .unwrap();
    assert_eq!(
        added,
        track_ids.len(),
        "the track the library lacks is left out"
    );

    let items = db.queue_value().await.unwrap().manual;
    let resolved: Vec<&str> = items.iter().map(|item| item.track_id.as_str()).collect();
    let expected: Vec<&str> = track_ids.iter().map(String::as_str).collect();
    assert_eq!(resolved, expected);
    assert!(items
        .iter()
        .all(|item| item.display.album_title == "Album Title A"));
}
