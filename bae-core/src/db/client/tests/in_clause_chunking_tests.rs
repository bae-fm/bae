use super::super::*;
use super::*;
use crate::playback::QueueEntryId;
use coven::SystemClock;
use std::sync::Arc;

async fn chunked_track_db() -> (Database, tempfile::TempDir, Vec<String>) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let track_count = SQL_MAX_IN_VARS * 45;
    let track_ids: Vec<String> = (0..track_count)
        .map(|index| bae_test_support::test_uuid(&format!("track-{index}")))
        .collect();
    let seed_track_ids = track_ids.clone();
    db.call(move |conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )?;
        // The ids are minted in Rust, not in SQL: coven takes only canonical
        // UUIDs on a synced row, and the test's assertions need the same
        // values the seed wrote.
        for (index, track_id) in seed_track_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO tracks \
                     (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at) \
                 VALUES (?1, '0252dedb-ee39-4547-8803-438dbeb57a64', ?2, 1, ?3, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z')",
                params![track_id, format!("Track Title {index}"), index as i64],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp, track_ids)
}

/// `cover_versions` takes a whole page's release ids, which is unbounded, so it
/// chunks like its siblings. Past SQLite's variable limit an unchunked `IN`
/// doesn't return fewer rows — it fails the query outright.
#[tokio::test]
async fn cover_versions_merges_chunks() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

    let cover_count = SQL_MAX_IN_VARS * 3;
    // Minted in Rust for the same reason as the track seed: coven takes only
    // canonical UUIDs on a synced row.
    let release_ids: Vec<String> = (0..cover_count)
        .map(|i| bae_test_support::test_uuid(&format!("release-{i}")))
        .collect();
    let seed_release_ids = release_ids.clone();
    db.call(move |conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, 'cdb9e2f2-ba4c-43ac-8422-765445141290', 0, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )?;
        // covers.id is a FK to releases.id, so every cover needs its release.
        for (index, release_id) in seed_release_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
                 VALUES (?1, 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z')",
                params![release_id],
            )?;
            conn.execute(
                "INSERT INTO covers \
                     (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at) \
                 VALUES (?1, ?2, 'image/jpeg', 1024, 'discogs', ?3, ?4, '2026-01-01T00:00:00Z')",
                params![
                    release_id,
                    bae_test_support::test_uuid(&format!("cover-blob-{index}")),
                    crate::util::fs::hash_bytes(b"fixture"),
                    format!("stamp-{index}")
                ],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();

    let versions = db.cover_versions(&release_ids).await.unwrap();

    // Spans three chunks; every row from every chunk must survive the merge.
    assert_eq!(versions.len(), cover_count);
    assert_eq!(
        versions.get(&release_ids[0]).map(String::as_str),
        Some("stamp-0")
    );
    let last = cover_count - 1;
    assert_eq!(
        versions.get(&release_ids[last]).map(String::as_str),
        Some(format!("stamp-{last}").as_str())
    );
}

#[tokio::test]
async fn track_id_queries_merge_chunks() {
    let (db, _tmp, track_ids) = chunked_track_db().await;
    let mut requested = track_ids.clone();
    requested.insert(SQL_MAX_IN_VARS / 2, "missing-track".to_string());

    let mut existing = db.filter_existing_track_ids(&requested).await.unwrap();
    existing.sort();
    let mut expected_existing = track_ids.clone();
    expected_existing.sort();
    assert_eq!(existing, expected_existing);

    let album_ids = db.get_album_ids_for_tracks(&requested).await.unwrap();
    assert_eq!(album_ids.len(), track_ids.len());
    for track_id in &track_ids {
        assert_eq!(album_ids.get(track_id).map(String::as_str), Some(ALBUM_A));
    }

    let entries: Vec<QueueEntry> = requested
        .iter()
        .enumerate()
        .map(|(index, track_id)| QueueEntry {
            id: QueueEntryId(format!("entry-{index}")),
            track_id: track_id.clone(),
        })
        .collect();
    let items = db.get_queue_items(&entries).await.unwrap();
    let resolved_track_ids: Vec<&str> = items.iter().map(|item| item.track_id.as_str()).collect();
    let expected_track_ids: Vec<&str> = requested
        .iter()
        .filter(|track_id| track_id.as_str() != "missing-track")
        .map(String::as_str)
        .collect();
    assert_eq!(resolved_track_ids, expected_track_ids);
}
