use super::*;
use crate::tables::tests::*;
use bae_records::LibraryImageType;
const RELEASE_LONELY: &str = "fcf4be32-159f-4790-87a1-697700a74462";
const RELEASE_OTHER: &str = "ce596bd7-be97-4416-8b6d-47f315bae466";
const TRACK_LONELY: &str = "03c41035-ce18-4fa0-8e83-c446df26a551";
const TRACK_OTHER: &str = "69e67928-545a-4dcf-8ae7-ef7778331231";
const ALBUM_NO_PRIMARY: &str = "c6648d5a-617e-4b69-87da-b7f1c4fb5e65";
const ALBUM_PRIMARY: &str = "82a53f44-1b76-435b-89f0-42749371ee15";
const RELEASE_PRIMARY: &str = "2ffa8060-aa00-4147-8ad4-f373ef66c407";

/// One album holds one release; the other holds two, and the queued track is
/// on the one that is NOT its primary.
async fn cover_db() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let artist = bae_records::DbArtist {
        id: ARTIST_A.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    for (album_id, title, releases) in [
        (ALBUM_NO_PRIMARY, "Album With No Primary", &[RELEASE_LONELY][..]),
        (ALBUM_PRIMARY, "Album With A Primary", &[RELEASE_PRIMARY, RELEASE_OTHER][..]),
    ] {
        let album = bae_records::DbAlbum {
            id: album_id.to_string(),
            year: Some(2026),
            ..bae_records::DbAlbum::new_test(title, &coven::UuidIds)
        };
        records.insert_album(&album).await.unwrap();
        records
            .insert_album_artist(&bae_records::DbAlbumArtist::new_test(album_id, ARTIST_A))
            .await
            .unwrap();
        for release_id in releases {
            records
                .insert_release(&bae_records::DbRelease::new_test(album_id, release_id))
                .await
                .unwrap();
        }
    }
    for (release_id, track_id, title) in [
        (RELEASE_LONELY, TRACK_LONELY, "Track On The Only Release"),
        (RELEASE_OTHER, TRACK_OTHER, "Track On The Non-Primary Release"),
    ] {
        records
            .insert_release_medium(&bae_records::DbReleaseMedium::new_test(release_id))
            .await
            .unwrap();
        let track = bae_records::DbTrack {
            duration_ms: 1000,
            ..bae_records::DbTrack::new_test(release_id, track_id, title, 1)
        };
        records.insert_track(&track).await.unwrap();
    }
    write_cover(&records, RELEASE_LONELY, "lonely release cover").await;
    write_cover(&records, RELEASE_OTHER, "other release cover").await;
    (db, tmp)
}

async fn cover_of(db: &Store, track_id: &str) -> Option<bae_records::ImageRef> {
    PlaybackQueueTables::new(db.clone()).clear_manual().await.unwrap();
    let items = queue_items(db, &[track_id]).await;
    assert_eq!(items.len(), 1);
    items[0].display.cover_image.clone()
}

/// The release's cover reference, carrying the version of the cover stored for it.
async fn cover_ref(db: &Store, release_id: &str) -> Option<bae_records::ImageRef> {
    let records = bae_records::test_fixtures::test_records(db);
    Some(bae_records::ImageRef {
        id: release_id.to_string(),
        version: stored_image_version(&records, bae_records::synced::COVERS_NAMESPACE, release_id)
            .await,
        image_type: LibraryImageType::Cover,
    })
}

/// A track on its album's only release shows that release's cover.
#[tokio::test]
async fn queue_row_covers_a_track_on_its_album_s_one_release() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(cover_of(&db, TRACK_LONELY).await, cover_ref(&db, RELEASE_LONELY).await,);
}

/// The album's primary release is set, but the queued track is on a different
/// one — the row must show the art of the release being played.
#[tokio::test]
async fn queue_row_covers_the_track_s_own_release_not_the_album_s_primary() {
    let (db, _tmp) = cover_db().await;
    assert_eq!(cover_of(&db, TRACK_OTHER).await, cover_ref(&db, RELEASE_OTHER).await,);
}

/// Replacing the bytes changes the queue and list image versions.
#[tokio::test]
async fn replacing_a_cover_changes_its_version() {
    let (db, _tmp) = cover_db().await;
    let records = bae_records::test_fixtures::test_records(&db);
    let before = cover_of(&db, TRACK_LONELY).await.expect("the release has a cover");
    let listed_before = records.cover_version(RELEASE_LONELY).await.unwrap();

    write_cover(&records, RELEASE_LONELY, "replacement cover").await;

    let after = cover_of(&db, TRACK_LONELY).await.expect("the release has a cover");
    assert_ne!(after.version, before.version, "the queue's cover version moves");
    assert_ne!(
        records.cover_version(RELEASE_LONELY).await.unwrap(),
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
    let display =
        track_display(&db, TRACK_OTHER).await.unwrap().expect("the library holds the track");
    assert_eq!(display.album_id, "82a53f44-1b76-435b-89f0-42749371ee15");
    assert_eq!(display.release_id, RELEASE_OTHER);
}
