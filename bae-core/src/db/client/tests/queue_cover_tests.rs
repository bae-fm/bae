use super::super::*;
use super::*;
use crate::playback::QueueEntryId;

/// One album holds one release; the other holds two, and the queued track is
/// on the one that is NOT its primary.
async fn cover_db() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db().await;
    db.call(move |conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, is_compilation, created_at) VALUES ('c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'Album With No Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, 0, '2026-01-01T00:00:00Z'),
             ('82a53f44-1b76-435b-89f0-42749371ee15', 'Album With A Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, 0, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('fcf4be32-159f-4790-87a1-697700a74462', 'c6648d5a-617e-4b69-87da-b7f1c4fb5e65', '2026-01-01T00:00:00Z', 0),
             ('2ffa8060-aa00-4147-8ad4-f373ef66c407', '82a53f44-1b76-435b-89f0-42749371ee15', '2026-01-01T00:00:00Z', 1),
             ('ce596bd7-be97-4416-8b6d-47f315bae466', '82a53f44-1b76-435b-89f0-42749371ee15', '2026-01-01T00:00:01Z', 2);

            INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES ('963cc1cc-0f18-5b6b-a604-fa66bfad6e3e', 'ce596bd7-be97-4416-8b6d-47f315bae466', 1, 'cd', 1, '2026-01-01T00:00:00Z'), ('9b885716-3e9d-5b85-8d85-8a84db7a0b6d', 'fcf4be32-159f-4790-87a1-697700a74462', 1, 'cd', 1, '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES ('03c41035-ce18-4fa0-8e83-c446df26a551', 'fcf4be32-159f-4790-87a1-697700a74462', 'Track On The Only Release', 1, 1, 1000, '2026-01-01T00:00:00Z', '9b885716-3e9d-5b85-8d85-8a84db7a0b6d'),
             ('69e67928-545a-4dcf-8ae7-ef7778331231', 'ce596bd7-be97-4416-8b6d-47f315bae466', 'Track On The Non-Primary Release', 1, 1, 1000, '2026-01-01T00:00:00Z', '963cc1cc-0f18-5b6b-a604-fa66bfad6e3e');

            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    super::write_cover(&db, RELEASE_LONELY, "lonely release cover").await;
    super::write_cover(&db, RELEASE_OTHER, "other release cover").await;
    (db, tmp)
}

async fn cover_of(db: &Database, track_id: &str) -> Option<crate::album_detail::ImageRef> {
    let items = super::queue_items(
        db,
        &[QueueEntry {
            id: QueueEntryId(format!("entry-{track_id}")),
            track_id: track_id.to_string(),
        }],
    )
    .await;
    assert_eq!(items.len(), 1);
    items[0].display.cover_image.clone()
}

/// The release's cover reference, carrying the version of the cover stored for it.
async fn cover_ref(db: &Database, release_id: &str) -> Option<crate::album_detail::ImageRef> {
    Some(crate::album_detail::ImageRef {
        id: release_id.to_string(),
        version: super::stored_image_version(db, crate::sync::COVERS_NAMESPACE, release_id).await,
        image_type: LibraryImageType::Cover,
    })
}

/// A track on its album's only release shows that release's cover.
#[tokio::test]
async fn queue_row_covers_a_track_on_its_album_s_one_release() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(
        cover_of(&db, TRACK_LONELY).await,
        cover_ref(&db, RELEASE_LONELY).await,
    );
}

/// The album's primary release is set, but the queued track is on a different
/// one — the row must show the art of the release being played.
#[tokio::test]
async fn queue_row_covers_the_track_s_own_release_not_the_album_s_primary() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(
        cover_of(&db, TRACK_OTHER).await,
        cover_ref(&db, RELEASE_OTHER).await,
    );
}

/// Replacing the bytes changes the queue and list image versions.
#[tokio::test]
async fn replacing_a_cover_changes_its_version() {
    let (db, _tmp) = cover_db().await;
    let before = cover_of(&db, TRACK_LONELY)
        .await
        .expect("the release has a cover");
    let listed_before = db.cover_version(RELEASE_LONELY).await.unwrap();

    super::write_cover(&db, RELEASE_LONELY, "replacement cover").await;

    let after = cover_of(&db, TRACK_LONELY)
        .await
        .expect("the release has a cover");
    assert_ne!(
        after.version, before.version,
        "the queue's cover version moves"
    );
    assert_ne!(
        db.cover_version(RELEASE_LONELY).await.unwrap(),
        listed_before,
        "the listed cover version moves"
    );
}

/// The album's primary release is set, but the track is on a different one —
/// the track's display names the release it is on, so a surface that reveals
/// the playing track opens that release rather than the album's default.
#[tokio::test]
async fn track_display_names_the_track_s_own_release_not_the_album_s_primary() {
    let (db, _tmp) = cover_db().await;
    let display = db
        .track_display(TRACK_OTHER)
        .await
        .unwrap()
        .expect("the library holds the track");
    assert_eq!(display.album_id, "82a53f44-1b76-435b-89f0-42749371ee15");
    assert_eq!(display.release_id, RELEASE_OTHER);
}
