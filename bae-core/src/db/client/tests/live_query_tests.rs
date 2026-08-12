use super::super::*;
use coven::SystemClock;
use std::sync::Arc;
use std::time::Duration;

const ARTIST_ID: &str = "96f0ef68-c284-4f74-b3f6-d9a4b48ee6d1";
const ALBUM_ID: &str = "975b724f-fce7-4fdb-85be-9f57fd9ba496";
const RELEASE_ID: &str = "44751464-1552-44be-8c5a-955cc5b61d12";
const OTHER_ALBUM_ID: &str = "5cf43aa4-8374-4ec4-bf08-239685a62f37";
const IDENTITY_ID: &str = "d9af7374-f7de-4d5b-8f9e-19b250ca2693";

async fn live_db() -> (Database, tempfile::TempDir) {
    let temp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        temp.path().join("library.db").to_str().unwrap(),
        Arc::new(SystemClock),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|sql| {
        sql.execute_batch(&format!(
            "INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'seed', '2026-01-01T00:00:00Z');
             INSERT INTO albums
               (id, title, artist_id, primary_release_id, is_compilation, _updated_at, created_at)
             VALUES ('{ALBUM_ID}', 'Album Title', '{ARTIST_ID}', '{RELEASE_ID}', 0, 'seed', '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
             VALUES ('{RELEASE_ID}', '{ALBUM_ID}', 'file_tags', 1, 'seed', '2026-01-01T00:00:00Z');"
        ))
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, temp)
}

#[tokio::test]
async fn album_page_subscription_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_page(&[], 0, 50);

    let initial = live.next().await.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.rows[0].title, "Album Title");
    assert!(initial.cover_versions.is_empty());

    let cover_hash = crate::util::fs::hash_bytes(b"cover fixture");
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO covers
             (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
             VALUES (?1, ?2, 'image/jpeg', 12, 'file_tags', ?3, 'cover-v1', '2026-01-01T00:00:00Z')",
            params![
                RELEASE_ID,
                "bd5c1f6c-3b6e-4d16-9f0a-2c1d5f61a0aa",
                cover_hash
            ],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("cover write wakes album page")
        .unwrap();
    assert_eq!(
        updated.cover_versions.get(RELEASE_ID).map(String::as_str),
        Some("cover-v1")
    );
}

#[tokio::test]
async fn album_page_subscription_ignores_an_unread_table() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_page(&[], 0, 50);
    live.next().await.unwrap();

    db.call(|sql| {
        sql.execute(
            "INSERT INTO playback_state
             (id, source, shuffled, manual, repeat, current_track_id, position_ms, volume, is_muted)
             VALUES ('current', NULL, NULL, '[]', 'off', NULL, 0, 1.0, 0)",
            [],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn album_detail_subscription_ignores_an_unread_column() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_detail(ALBUM_ID);
    live.next().await.unwrap();

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET _updated_at = 'unread-column-write' WHERE id = ?1",
            params![ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db().await;
    db.call(|sql| {
        sql.execute(
            "INSERT INTO albums
             (id, title, artist_id, is_compilation, _updated_at, created_at)
             VALUES (?1, 'Other Album', ?2, 0, 'seed', '2026-01-01T00:00:00Z')",
            params![OTHER_ALBUM_ID, ARTIST_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let mut live = db.inner.handle.subscribe(|sql| {
        sql.query_row(
            "SELECT title FROM albums WHERE id = ?1",
            params![ALBUM_ID],
            |row| row.get::<_, String>(0),
        )
        .map_err(CovenError::from)
    });
    assert_eq!(live.next().await.unwrap(), "Album Title");

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_detail(ALBUM_ID);
    assert!(live.next().await.unwrap().detail.is_some());

    db.call(|sql| {
        sql.execute("DELETE FROM albums WHERE id = ?1", params![ALBUM_ID])
            .map(|_| ())
            .map_err(DbError::from)
    })
    .await
    .unwrap();

    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("album deletion wakes album detail")
        .unwrap();
    assert!(deleted.detail.is_none());
}

#[tokio::test]
async fn release_library_status_subscription_delivers_identity_changes() {
    let (db, _temp) = live_db().await;
    let check = LibraryCheck {
        release_id: "source-release-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("source-group-1".to_string()),
    };
    let mut live = db.subscribe_release_library_status(check);

    let initial = live.next().await.unwrap();
    assert!(!initial.release_in_library);
    assert!(!initial.album_in_library);

    db.call(|sql| {
        sql.execute(
            "INSERT INTO release_identities
             (id, release_id, source, source_release_id, source_group_id, _updated_at, created_at)
             VALUES (?1, ?2, 'musicbrainz', 'source-release-1', 'source-group-1', 'identity-v1', '2026-01-01T00:00:00Z')",
            params![IDENTITY_ID, RELEASE_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity write wakes release library status")
        .unwrap();
    assert!(updated.release_in_library);
    assert!(updated.album_in_library);
    assert_eq!(updated.album_id.as_deref(), Some(ALBUM_ID));
}
