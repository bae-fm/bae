//! These tests check where core places an album's card in the grid, read
//! live. Each test writes its library through the records domain's own
//! writes.

use crate::album::AlbumPlace;
use crate::*;

use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

/// The artist who made every album of the test's library.
const ARTIST: &str = "20000000-0000-4000-8000-000000000001";

/// Album `index` of the test's library.
fn album(index: u32) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

/// The open album's place follows a write that moves its card without
/// changing how many cards the grid has: renaming an album above it under a
/// title sort moves the card up a place.
#[tokio::test]
async fn an_albums_place_follows_a_rename_that_moves_its_card() {
    let (db, _dir) = temp_db(crate::TEST_SCHEMA).await;
    let mut library = RecordsFixture::new().artist(ARTIST, "Artist Name A");
    for (title, index) in ["Album A", "Album B", "Album C"].into_iter().zip(0..) {
        let release = format!("10000000-0000-4000-8000-{index:012}");
        library =
            library.album(&album(index), title, None, &[ARTIST]).release(&album(index), &release);
    }
    library.write(&test_records(&db)).await;
    let views = LibraryViews::new(db.clone());
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let mut place = views.subscribe_album_place(&sort, &album(2), false, None);
    let first = place.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(first, Some(AlbumPlace { position: 2, section_id: None }));

    test_records(&db).rename_album_for_test(&album(0), "Album Z").await.unwrap();
    let moved = tokio::time::timeout(HANG_GUARD, place.next_inline())
        .await
        .expect("the rename reruns the place")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(moved, Some(AlbumPlace { position: 1, section_id: None }));
}
