use super::super::*;
use super::*;
use crate::playback::QueueEntryId;
use coven::SystemClock;
use std::sync::Arc;

/// `album-null` has no primary release at all; `album-set` has one, pointing at
/// a release that the queued track is NOT on.
async fn cover_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    // coven verifies a blob row's declared hash, so the seeded covers carry
    // real content hashes rather than placeholder strings.
    let lonely_hash = crate::util::fs::hash_bytes(b"cover-lonely");
    let other_hash = crate::util::fs::hash_bytes(b"cover-other");
    db.call(move |conn| {
        conn.execute_batch(
            &format!(
            "
            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'Album With No Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                ('82a53f44-1b76-435b-89f0-42749371ee15', 'Album With A Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, '2ffa8060-aa00-4147-8ad4-f373ef66c407', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('fcf4be32-159f-4790-87a1-697700a74462', 'c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('2ffa8060-aa00-4147-8ad4-f373ef66c407', '82a53f44-1b76-435b-89f0-42749371ee15', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('ce596bd7-be97-4416-8b6d-47f315bae466', '82a53f44-1b76-435b-89f0-42749371ee15', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
            VALUES
                ('03c41035-ce18-4fa0-8e83-c446df26a551', 'fcf4be32-159f-4790-87a1-697700a74462', 'Track On The Only Release', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('69e67928-545a-4dcf-8ae7-ef7778331231', 'ce596bd7-be97-4416-8b6d-47f315bae466', 'Track On The Non-Primary Release', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO covers (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
            VALUES
                ('fcf4be32-159f-4790-87a1-697700a74462', 'bd5c1f6c-3b6e-4d16-9f0a-2c1d5f61a0aa', 'image/jpeg', 1024, 'discogs', '{lonely_hash}', 'cover-stamp-lonely', '2026-01-01T00:00:00Z'),
                ('ce596bd7-be97-4416-8b6d-47f315bae466', '0f2b9a51-7d2c-4a2f-8f16-9c0a3f1b2d44', 'image/jpeg', 1024, 'discogs', '{other_hash}', 'cover-stamp-other', '2026-01-01T00:00:00Z');
            "
            ),
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

async fn cover_of(db: &Database, track_id: &str) -> Option<crate::album_detail::ImageRef> {
    let items = db
        .get_queue_items(&[QueueEntry {
            id: QueueEntryId(format!("entry-{track_id}")),
            track_id: track_id.to_string(),
        }])
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    items[0].cover_image.clone()
}

fn cover_ref(release_id: &str, version: &str) -> Option<crate::album_detail::ImageRef> {
    Some(crate::album_detail::ImageRef {
        id: release_id.to_string(),
        version: version.to_string(),
        image_type: LibraryImageType::Cover,
    })
}

/// `albums.primary_release_id` is NULL, so reading it raw yields no cover at
/// all and the queue row renders art-less.
#[tokio::test]
async fn queue_row_covers_a_track_whose_album_has_no_primary_release() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(
        cover_of(&db, TRACK_LONELY).await,
        cover_ref(RELEASE_LONELY, "cover-stamp-lonely"),
    );
}

/// The album's primary release is set, but the queued track is on a different
/// one — the row must show the art of the release being played.
#[tokio::test]
async fn queue_row_covers_the_track_s_own_release_not_the_album_s_primary() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(
        cover_of(&db, TRACK_OTHER).await,
        cover_ref(RELEASE_OTHER, "cover-stamp-other"),
    );
}
