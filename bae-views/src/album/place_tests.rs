//! These tests check where core places an album's card in the grid, read
//! live, and which albums a shift-click selects between two cards. Each test
//! writes its library through the records domain's own writes.

use crate::album::{AlbumCard, AlbumPlace};
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

/// The second artist of the grouped library.
const OTHER_ARTIST: &str = "20000000-0000-4000-8000-000000000002";

/// Album 0 by the first artist, album 1 by both artists and album 2 by the
/// second artist, titled in that order: grouped by artist under a title
/// sort, the grid's cards are album 0 and album 1 in the first artist's
/// group, then album 1 and album 2 in the second's.
async fn shared_album_library() -> (Store, tempfile::TempDir) {
    let (db, dir) = temp_db(crate::TEST_SCHEMA).await;
    let credits: [&[&str]; 3] = [&[ARTIST], &[ARTIST, OTHER_ARTIST], &[OTHER_ARTIST]];
    let mut library =
        RecordsFixture::new().artist(ARTIST, "Artist Name A").artist(OTHER_ARTIST, "Artist Name B");
    for (index, artists) in (0..).zip(credits) {
        let release = format!("10000000-0000-4000-8000-{index:012}");
        library = library
            .album(&album(index), &format!("Album {index}"), None, artists)
            .release(&album(index), &release);
    }
    library.write(&test_records(&db)).await;
    (db, dir)
}

/// The albums between two cards, as a shift-click reads them.
async fn between(
    db: &Store,
    group_by_artist: bool,
    from: (&str, Option<&str>),
    to: (&str, Option<&str>),
) -> Vec<String> {
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let card = |(album_id, section_id)| AlbumCard { album_id, section_id };
    LibraryViews::new(db.clone())
        .album_ids_between(&sort, group_by_artist, card(from), card(to))
        .await
        .unwrap()
}

/// A shift-click selects the albums of the cards from one card to the
/// other in the grid's order, whichever comes first, each album once, from
/// the card in the group it was clicked in.
#[tokio::test]
async fn the_albums_between_two_cards_are_those_of_the_cards_between_them() {
    let (db, _dir) = shared_album_library().await;
    let (first, shared, last) = (album(0), album(1), album(2));
    assert_eq!(
        between(&db, false, (&first, None), (&last, None)).await,
        [first.clone(), shared.clone(), last.clone()]
    );
    assert_eq!(
        between(&db, false, (&last, None), (&shared, None)).await,
        [shared.clone(), last.clone()]
    );
    // Grouped, the shared album's card in the second artist's group comes
    // after the first artist's group.
    assert_eq!(
        between(&db, true, (&shared, Some(OTHER_ARTIST)), (&last, None)).await,
        [shared.clone(), last.clone()]
    );
    assert_eq!(
        between(&db, true, (&first, None), (&shared, Some(OTHER_ARTIST))).await,
        [first.clone(), shared.clone()]
    );
    assert_eq!(
        between(&db, true, (&shared, Some(ARTIST)), (&last, Some(OTHER_ARTIST))).await,
        [shared.clone(), last.clone()],
        "an album with two cards between the ends is selected once"
    );
}

/// With no card of the album a shift-click extends from, it selects the
/// clicked album alone; with no card of the clicked album, nothing.
#[tokio::test]
async fn the_albums_between_a_missing_card_and_another_are_the_others_alone() {
    let (db, _dir) = shared_album_library().await;
    let (first, last) = (album(0), album(2));
    assert_eq!(between(&db, false, ("no-such-album", None), (&last, None)).await, [last]);
    assert!(between(&db, false, (&first, None), ("no-such-album", None)).await.is_empty());
}
