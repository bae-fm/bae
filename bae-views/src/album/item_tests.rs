//! A media browser's reads of one album and of one track.

use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::*;

const PAGED_ARTIST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5021";
const PLAYABLE_ALBUM: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5011";
const PLAYABLE_RELEASE: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5012";
const EMPTY_RELEASE: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5002";
const PICKED_TRACK: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5043";
const OTHER_TRACK: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5044";

/// `find_album_item` returns an album the grid lists with its primary
/// release's cover, and none for an unknown album.
#[tokio::test]
async fn an_album_item_is_read_by_its_id() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(PAGED_ARTIST, "Paged Artist")
        .album(PLAYABLE_ALBUM, "Picked Album", Some(2026), &[PAGED_ARTIST])
        .release(PLAYABLE_ALBUM, PLAYABLE_RELEASE)
        .write(&test_records(&db))
        .await;
    write_cover(&test_records(&db), PLAYABLE_RELEASE, "cover bytes").await;
    let views = crate::LibraryViews::new(db.clone());

    let album = views.find_album_item(PLAYABLE_ALBUM).await.unwrap().expect("album");
    assert_eq!(album.title, "Picked Album");
    assert_eq!(album.cover.map(|cover| cover.id), Some(PLAYABLE_RELEASE.to_string()));
    assert!(views.find_album_item(PLAYABLE_RELEASE).await.unwrap().is_none());
}

/// `find_release_track` returns the browse item for a track by its release
/// and track ids, and none when the track is on another release.
#[tokio::test]
async fn a_release_track_is_read_by_its_ids() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(PAGED_ARTIST, "Paged Artist")
        .album(PLAYABLE_ALBUM, "Picked Album", Some(2026), &[PAGED_ARTIST])
        .release(PLAYABLE_ALBUM, PLAYABLE_RELEASE)
        .tracks(PLAYABLE_RELEASE, &[(PICKED_TRACK, "Picked"), (OTHER_TRACK, "Other")])
        .write(&test_records(&db))
        .await;
    let views = crate::LibraryViews::new(db.clone());

    write_cover(&test_records(&db), PLAYABLE_RELEASE, "cover bytes").await;
    let picked = views.find_release_track(PLAYABLE_RELEASE, PICKED_TRACK).await.unwrap();
    assert_eq!(
        picked.as_ref().map(|track| (
            track.artist_line.clone(),
            track.cover.as_ref().map(|cover| cover.id.clone()),
        )),
        Some((
            bae_types::credits::CreditLine::of_name("Paged Artist"),
            Some(PLAYABLE_RELEASE.to_string()),
        ))
    );
    assert_eq!(
        picked.map(|track| (track.id, track.title, track.release_id)),
        Some((PICKED_TRACK.to_string(), "Picked".to_string(), PLAYABLE_RELEASE.to_string(),))
    );
    assert!(views.find_release_track(EMPTY_RELEASE, PICKED_TRACK).await.unwrap().is_none());
}
