//! An album's tracks as a media browser lists them: its main release's, in
//! playing order, read a page at a time from a track's key on, over
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

/// Each track's id and title in one delivery of a page of the album's
/// tracks.
fn tracks_of(rows: crate::ListRows) -> Vec<(String, String)> {
    let crate::ListRows::AlbumTracks(rows) = rows else {
        panic!("a page of an album's tracks holds tracks");
    };
    rows.into_iter().map(|track| (track.id, track.title)).collect()
}

fn track(id: &str, title: &str) -> (String, String) {
    (id.to_string(), title.to_string())
}

/// A page holds its limit of the main release's tracks from the track at its
/// first place, in playing order, never another release's.
#[tokio::test]
async fn an_albums_tracks_are_its_main_releases_in_pages() {
    let (views, _db, _tmp) = paged_album().await;
    let page = |offset| {
        crate::test_pages::page_values(
            views.subscribe_album_track_page(ALBUM, bae_store::ListPage { offset, limit: 2 }),
        )
    };

    let rows = crate::test_pages::next_page(&page(0)).await;
    assert_eq!(tracks_of(rows), [track(FIRST, "First"), track(SECOND, "Second")]);
    let rows = crate::test_pages::next_page(&page(2)).await;
    assert_eq!(tracks_of(rows), [track(THIRD, "Third")]);
    let rows = crate::test_pages::next_page(&page(3)).await;
    assert_eq!(tracks_of(rows), Vec::<(String, String)>::new());
}

/// A page of the album's tracks follows a rename of a track inside it, and
/// the count of the tracks follows the album to the release that becomes its
/// main one when the test deletes the first release.
#[tokio::test]
async fn a_page_follows_a_renamed_track_and_the_count_follows_the_main_release() {
    let (views, db, _tmp) = paged_album().await;
    let live = crate::test_pages::page_values(
        views.subscribe_album_track_page(ALBUM, bae_store::ListPage { offset: 0, limit: 3 }),
    );
    crate::test_pages::next_page(&live).await;

    test_records(&db).rename_track_for_test(SECOND, "Second Renamed").await.unwrap();
    assert_eq!(
        tracks_of(crate::test_pages::next_page(&live).await),
        [track(FIRST, "First"), track(SECOND, "Second Renamed"), track(THIRD, "Third")]
    );

    let mut count = views.subscribe_album_track_count(ALBUM);
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 3);
    delete_release(&test_records(&db), MAIN_RELEASE).await;
    let counted =
        tokio::time::timeout(HANG_GUARD, count.next_inline()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 1);
}
