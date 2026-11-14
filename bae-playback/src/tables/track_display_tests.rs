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

const INFO_ALBUM: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b01";
const INFO_RECORD: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b02";
const INFO_DISCS: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b03";
const INFO_SIDE_A: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b11";
const INFO_SIDE_B: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b12";
const INFO_SECOND_DISC: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b13";
const INFO_SIDE_C: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b15";
const INFO_GONE: &str = "e1a5c9b6-7f83-4a0c-9b42-5d6e7f8a9b14";

/// The side facts of the tracks asked for come back in the order asked, as one
/// list across releases: a record's sides are lettered and a pause at each is
/// a side's, a CD's disc is numbered and a pause there is a disc's, and a
/// track the library does not hold is left out.
#[tokio::test]
async fn track_infos_follow_the_ids_asked_across_releases_and_leave_out_the_missing() {
    use bae_types::mediums::ReleaseMedium;
    use bae_types::pressing::Medium;
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let medium = |release: &str, position: u16, carrier, sides: u8| {
        let mut row = bae_records::DbReleaseMedium::new_test(release);
        row.id = format!("{release}-medium-{position}");
        row.position = std::num::NonZeroU16::new(position).unwrap();
        row.medium = ReleaseMedium {
            carrier: Some(carrier),
            sides: std::num::NonZeroU8::new(sides).unwrap(),
        };
        row
    };
    bae_records::test_fixtures::library::RecordsFixture::new()
        .artist(ARTIST_A, "Artist Name")
        .album(INFO_ALBUM, "Album Title", None, &[ARTIST_A])
        .release(INFO_ALBUM, INFO_RECORD)
        .tracks(
            INFO_RECORD,
            &[
                (INFO_SIDE_A, "Side A Track"),
                (INFO_SIDE_B, "Side B Track"),
                (INFO_SIDE_C, "Side C Track"),
            ],
        )
        .release(INFO_ALBUM, INFO_DISCS)
        .tracks(INFO_DISCS, &[(INFO_SECOND_DISC, "Second Disc Track")])
        .edit(|rows| {
            rows.mediums = vec![
                medium(INFO_RECORD, 1, Medium::Vinyl, 2),
                medium(INFO_RECORD, 2, Medium::Vinyl, 1),
                medium(INFO_DISCS, 1, Medium::Cd, 1),
                medium(INFO_DISCS, 2, Medium::Cd, 1),
            ];
            for track in &mut rows.tracks {
                let (position, side, medium) = match track.id.as_str() {
                    INFO_SIDE_A => (INFO_RECORD, 1, 1),
                    INFO_SIDE_B => (INFO_RECORD, 2, 1),
                    INFO_SIDE_C => (INFO_RECORD, 1, 2),
                    _ => (INFO_DISCS, 1, 2),
                };
                track.medium_id = format!("{position}-medium-{medium}");
                track.side = std::num::NonZeroU8::new(side).unwrap();
            }
        })
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;

    let mut follow = subscribe_playback_track_infos(
        &db,
        vec![
            INFO_SECOND_DISC.to_string(),
            INFO_GONE.to_string(),
            INFO_SIDE_B.to_string(),
            INFO_SIDE_C.to_string(),
            INFO_SIDE_A.to_string(),
        ],
    );
    let infos = follow.next().await.expect("the read runs").result.unwrap();

    let facts: Vec<_> = infos
        .iter()
        .map(|info| {
            let side = info.side.as_ref().expect("a carrier with sides");
            (
                info.track_id.as_str(),
                side.medium.get(),
                side.side.get(),
                side.boundary,
                &side.label[..],
            )
        })
        .collect();
    use crate::PlaybackPauseBoundary::{Disc, Side};
    assert_eq!(
        facts,
        [
            (INFO_SECOND_DISC, 2, 1, Disc, "2"),
            (INFO_SIDE_B, 1, 2, Side, "B"),
            (INFO_SIDE_C, 2, 1, Side, "C"),
            (INFO_SIDE_A, 1, 1, Side, "A"),
        ]
    );
}
