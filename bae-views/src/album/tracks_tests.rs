//! An album's tracks as a media browser lists them: its main release's, in
//! playing order, read as a window from a track's key on, over
//! libraries written through the records domain's own writes.

use super::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::{delete_release, test_records};
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const COMPOSER: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c01";
const ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c51";
const MAIN_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c52";
const OTHER_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c53";
const FIRST: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c54";
const SECOND: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c55";
const THIRD: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c56";

/// An album with three tracks on its main release and one on another.
async fn paged_album() -> (LibraryViews, bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(COMPOSER, "Composer Name")
        .album(ALBUM, "Paged Album", Some(2026), &[COMPOSER])
        .release(ALBUM, MAIN_RELEASE)
        .tracks(MAIN_RELEASE, &[(FIRST, "First"), (SECOND, "Second"), (THIRD, "Third")])
        .release(ALBUM, OTHER_RELEASE)
        .tracks(OTHER_RELEASE, &[("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c57", "Other")])
        .write(&test_records(&db))
        .await;
    (LibraryViews::new(db.clone()), db, tmp)
}

/// Each track's id and title in one delivery of a window of the album's
/// tracks.
fn tracks_of(read: WindowRead<DbTrackSearchResult>) -> Vec<(String, String)> {
    let crate::ListRows::AlbumTracks(rows) = resolve_album_track_window(read).rows else {
        panic!("a window of an album's tracks holds tracks");
    };
    rows.into_iter().map(|track| (track.id, track.title)).collect()
}

fn track(id: &str, title: &str) -> (String, String) {
    (id.to_string(), title.to_string())
}

/// A window holds its limit of the main release's tracks from a track's key
/// on, in playing order, never another release's.
#[tokio::test]
async fn an_albums_tracks_are_its_main_releases_in_windows_from_a_key() {
    let (views, _db, _tmp) = paged_album().await;

    let mut live = views.subscribe_album_track_window(ALBUM, ListWindow::top(2));
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(tracks_of(read), [track(FIRST, "First"), track(SECOND, "Second")]);

    let third = views.album_track_key_at(ALBUM, 2).await.unwrap().expect("a third track");
    let mut live =
        views.subscribe_album_track_window(ALBUM, ListWindow { from: Some(third), limit: 2 });
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(tracks_of(read), [track(THIRD, "Third")]);

    let second = views.album_track_key_at(ALBUM, 1).await.unwrap().expect("a second track");
    let mut position = views.subscribe_album_track_position(ALBUM, second);
    assert_eq!(position.next_inline().await.expect("the query stays open").result.unwrap(), 1);
}

/// A window of the album's tracks follows a rename of a track inside it, and
/// the count of the tracks follows the album to the release that becomes its
/// main one when the test deletes the first release.
#[tokio::test]
async fn a_window_follows_a_renamed_track_and_the_count_follows_the_main_release() {
    let (views, db, _tmp) = paged_album().await;
    let mut live = views.subscribe_album_track_window(ALBUM, ListWindow::top(3));
    live.next_inline().await.expect("the query stays open").result.unwrap();

    test_records(&db).rename_track_for_test(SECOND, "Second Renamed").await.unwrap();
    let read = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap().result;
    assert_eq!(
        tracks_of(read.unwrap()),
        [track(FIRST, "First"), track(SECOND, "Second Renamed"), track(THIRD, "Third")]
    );

    let mut count = views.subscribe_album_track_count(ALBUM);
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 3);
    delete_release(&test_records(&db), MAIN_RELEASE).await;
    let counted =
        tokio::time::timeout(HANG_GUARD, count.next_inline()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 1);
}
