use super::super::*;
use crate::playback::QueueEntryId;

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

/// `cover_versions` takes a whole page's release ids, which is unbounded, so it
/// chunks like its siblings. Past SQLite's variable limit an unchunked `IN`
/// doesn't return fewer rows — it fails the query outright.
#[tokio::test]
async fn cover_versions_merges_chunks() {
    let (db, _tmp) = super::temp_db().await;

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
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'artist name primary', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, is_compilation, created_at) VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, 0, '2026-01-01T00:00:00Z');
            ",
        )?;
        // covers.id is a FK to releases.id, so every cover needs its release.
        for release_id in &seed_release_ids {
            conn.execute(
                "INSERT INTO releases (id, album_id, created_at, album_order) VALUES (?1, 'a67c03ad-425f-45e9-8279-0144c852aaa5', '2026-01-01T00:00:00Z', 0)",
                params![release_id],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();

    // Each cover holds its own bytes, so each carries its own version.
    let images: Vec<_> = release_ids
        .iter()
        .enumerate()
        .map(|(index, release_id)| {
            let bytes = format!("cover-{index}").into_bytes();
            let image = DbLibraryImage::cover(
                release_id,
                "local",
                None,
                &bytes,
                chrono::Utc::now(),
                uuid::Uuid::new_v4().to_string(),
            );
            (image, bytes)
        })
        .collect();
    let files = images.clone();
    Box::pin(db.inner.handle.write_with_files(
        move |writer| {
            for (image, bytes) in files {
                writer.put_file(crate::sync::COVERS_NAMESPACE, image.id, bytes);
            }
            Ok(())
        },
        move |sql| {
            for (image, _) in images {
                super::super::write::upsert_library_image_row(&sql, &image)
                    .map_err(CovenError::from)?;
            }
            Ok(())
        },
    ))
    .await
    .unwrap();

    let versions = db.cover_versions(&release_ids).await.unwrap();

    // Spans three chunks; every row from every chunk must survive the merge.
    assert_eq!(versions.len(), cover_count);
    for index in [0, cover_count - 1] {
        assert_eq!(
            versions.get(&release_ids[index]),
            Some(
                &super::stored_image_version(
                    &db,
                    crate::sync::COVERS_NAMESPACE,
                    &release_ids[index]
                )
                .await
            )
        );
    }
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

    let entries: Vec<QueueEntry> = requested
        .iter()
        .enumerate()
        .map(|(index, track_id)| QueueEntry {
            id: QueueEntryId(format!("entry-{index}")),
            track_id: track_id.clone(),
        })
        .collect();
    let items = super::queue_items(&db, &entries).await;
    let resolved_track_ids: Vec<&str> = items.iter().map(|item| item.track_id.as_str()).collect();
    let expected_track_ids: Vec<&str> = requested
        .iter()
        .filter(|track_id| track_id.as_str() != "missing-track")
        .map(String::as_str)
        .collect();
    assert_eq!(resolved_track_ids, expected_track_ids);
    // Every chunk's rows carry what the track → release → album join resolved,
    // so a dropped chunk shows up as a missing album, not just a missing id.
    assert!(items
        .iter()
        .all(|item| item.display.album_title == "Album Title A"));
}
