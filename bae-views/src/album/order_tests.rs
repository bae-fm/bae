//! A stacked sort of the album grid: every window, page, position, key, place
//! and selection of it follows each sort in turn, ties going by album id.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;
use bae_store::ListWindow;

const ARTIST_FIRST: &str = "30000000-0000-4000-8000-000000000001";
const ARTIST_SECOND: &str = "30000000-0000-4000-8000-000000000002";

/// Albums that tie on every field with another, so each place of a stacked
/// sort tells some two apart: title, year (none for some) and credits (none
/// for some).
const ALBUMS: [(&str, Option<i32>, &[&str]); 12] = [
    ("Album A", Some(2001), &[ARTIST_FIRST]),
    ("Album A", Some(2001), &[ARTIST_SECOND]),
    ("Album A", Some(2002), &[ARTIST_FIRST]),
    ("Album A", None, &[ARTIST_FIRST, ARTIST_SECOND]),
    ("Album B", Some(2001), &[ARTIST_FIRST]),
    ("Album B", Some(2002), &[ARTIST_SECOND]),
    ("Album B", None, &[]),
    ("Album B", Some(2001), &[]),
    ("Album C", Some(2002), &[ARTIST_FIRST, ARTIST_SECOND]),
    ("Album C", None, &[ARTIST_SECOND]),
    ("Album C", Some(2001), &[ARTIST_FIRST]),
    ("Album C", Some(2002), &[ARTIST_FIRST]),
];

fn album_id(index: usize) -> String {
    format!("40000000-0000-4000-8000-{index:012}")
}

/// The day album `index` of [`ALBUMS`] is added on: two albums each day, so
/// the date added ties them too.
fn added(index: usize) -> usize {
    index / 2
}

async fn tied_library() -> (Store, bae_util::test_files::TestFolder) {
    let (db, dir) = temp_db(crate::TEST_SCHEMA).await;
    let mut library = RecordsFixture::new()
        .artist(ARTIST_FIRST, "Artist Name A")
        .artist(ARTIST_SECOND, "Artist Name B");
    for (index, (title, year, artists)) in ALBUMS.into_iter().enumerate() {
        let release = format!("50000000-0000-4000-8000-{index:012}");
        library = library
            .at(&format!("2026-01-0{}T00:00:00Z", added(index) + 1))
            .album(&album_id(index), title, year, artists)
            .release(&album_id(index), &release);
    }
    library.write(&test_records(&db)).await;
    (db, dir)
}

/// Every stack of distinct fields, each either way, the empty one among them.
fn stacks() -> Vec<Vec<AlbumSortCriterion>> {
    let fields = [
        AlbumSortField::Title,
        AlbumSortField::Artist,
        AlbumSortField::Year,
        AlbumSortField::DateAdded,
    ];
    let mut stacks = vec![Vec::new()];
    let mut grown = 0;
    while grown < stacks.len() {
        let stack: Vec<AlbumSortCriterion> = stacks[grown].clone();
        grown += 1;
        for field in fields {
            if stack.iter().any(|taken| taken.field == field) {
                continue;
            }
            for direction in [SortDirection::Ascending, SortDirection::Descending] {
                let mut longer = stack.clone();
                longer.push(AlbumSortCriterion { field, direction });
                stacks.push(longer);
            }
        }
    }
    stacks
}

/// A stack of sorts and the albums of [`ALBUMS`], by index, in the order the
/// grid lists them under it.
type OrderedStack = (&'static [(AlbumSortField, SortDirection)], &'static [usize]);

/// Stacks of one to four sorts, and none, over the flat grid. An unknown year
/// sorts after every year, an album with no credits before every credit list,
/// the credit lists then as A, A and B, B, and albums the stack ties by id.
const FLAT_ORDERS: &[OrderedStack] = {
    use AlbumSortField::{Artist, DateAdded, Title, Year};
    use SortDirection::{Ascending as Up, Descending as Down};
    &[
        // With no sort, the newest first.
        (&[], &[10, 11, 8, 9, 6, 7, 4, 5, 2, 3, 0, 1]),
        (&[(Title, Up)], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        (&[(DateAdded, Up)], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
        (&[(Title, Down), (Year, Up)], &[10, 8, 11, 9, 4, 7, 5, 6, 0, 1, 2, 3]),
        (&[(Year, Down), (Artist, Up)], &[6, 3, 9, 2, 11, 8, 5, 7, 0, 4, 10, 1]),
        (&[(Artist, Down), (Title, Up)], &[1, 5, 9, 3, 8, 0, 2, 4, 10, 11, 6, 7]),
        (&[(Year, Up), (DateAdded, Down)], &[10, 7, 4, 0, 1, 11, 8, 5, 2, 9, 6, 3]),
        (
            &[(Artist, Up), (Year, Down), (Title, Down), (DateAdded, Up)],
            &[6, 7, 11, 2, 10, 4, 0, 3, 8, 9, 5, 1],
        ),
    ]
};

/// A stack over the grid grouped by artist: the artists descending, B's group
/// first, each group by year and then title descending.
const GROUPED_ORDER: OrderedStack = {
    use AlbumSortField::{Artist, Title, Year};
    use SortDirection::{Ascending as Up, Descending as Down};
    (&[(Artist, Down), (Year, Up), (Title, Down)], &[1, 8, 5, 9, 3, 10, 4, 0, 8, 11, 2, 3])
};

fn sort_of(stack: &[(AlbumSortField, SortDirection)]) -> Vec<AlbumSortCriterion> {
    stack.iter().map(|&(field, direction)| AlbumSortCriterion { field, direction }).collect()
}

fn ids_of(albums: &[usize]) -> Vec<String> {
    albums.iter().copied().map(album_id).collect()
}

#[tokio::test]
async fn a_stacked_sort_orders_the_grid_by_each_sort_in_turn() {
    let (db, _dir) = tied_library().await;
    let views = LibraryViews::new(db.clone());
    for (stack, albums) in FLAT_ORDERS {
        let listed: Vec<String> = views
            .get_albums(&sort_of(stack))
            .await
            .unwrap()
            .into_iter()
            .map(|album| album.id)
            .collect();
        assert_eq!(listed, ids_of(albums), "{stack:?}");
    }
    let (stack, albums) = GROUPED_ORDER;
    let grouped: Vec<String> = window_of(&views, &sort_of(stack), true, ListWindow::top(100))
        .await
        .into_iter()
        .map(|(id, _key)| id)
        .collect();
    assert_eq!(grouped, ids_of(albums), "{stack:?} grouped");
}

/// The rows of the grid's window of `limit` rows from `window`'s key.
async fn window_of(
    views: &LibraryViews,
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
    window: ListWindow,
) -> Vec<(String, OrderKey)> {
    views
        .subscribe_album_window(sort, group_by_artist, window)
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .rows
        .into_iter()
        .map(|(row, key)| (row.album.id, key))
        .collect()
}

/// Under every stacked sort, flat and grouped by artist, the grid read from
/// its top is its rows in order; a window from a row's key holds the rows
/// from it on; the key at a position, the rows before a key, an album's place,
/// a shift-click's range and a page all name the same positions.
#[tokio::test]
async fn every_read_of_a_stacked_sort_agrees_with_its_window_from_the_top() {
    let (db, _dir) = tied_library().await;
    let views = LibraryViews::new(db.clone());
    for group_by_artist in [false, true] {
        for sort in stacks() {
            let what = format!("{sort:?} grouped {group_by_artist}");
            let rows = window_of(&views, &sort, group_by_artist, ListWindow::top(100)).await;
            let ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
            // Albums with no credits are in no artist's group, and an album
            // with two is in two.
            let cards: usize = if group_by_artist {
                ALBUMS.iter().map(|album| album.2.len()).sum()
            } else {
                ALBUMS.len()
            };
            assert_eq!(rows.len(), cards, "{what}");
            for at in [0, 1, 5, rows.len() - 1] {
                let key =
                    views.album_key_at(&sort, group_by_artist, at as u64).await.unwrap().unwrap();
                assert_eq!(key, rows[at].1, "{what} key at {at}");
                let window = ListWindow { from: Some(key.clone()), limit: 3 };
                let held: Vec<String> = window_of(&views, &sort, group_by_artist, window)
                    .await
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                assert_eq!(held, ids[at..ids.len().min(at + 3)], "{what} window at {at}");
                let before = views
                    .subscribe_album_position(&sort, group_by_artist, key)
                    .next_inline()
                    .await
                    .expect("the query stays open")
                    .result
                    .unwrap();
                assert_eq!(before, at as u64, "{what} before {at}");
            }
            let past_end = views.album_key_at(&sort, group_by_artist, 1000).await.unwrap();
            assert_eq!(past_end.as_ref(), rows.last().map(|(_, key)| key), "{what} past end");

            // An album's place is its first card, and a range between two
            // cards names the albums between them, each once.
            let (first_id, last_id) = (ids[2], ids[ids.len() - 3]);
            let place = crate::tests::place_of(&views, &sort, last_id, group_by_artist, None)
                .await
                .unwrap();
            let last_at = ids.iter().position(|id| *id == last_id).unwrap();
            assert_eq!(place.position, last_at as u64, "{what} place");
            let first_at = ids.iter().position(|id| *id == first_id).unwrap();
            let mut between: Vec<&str> = Vec::new();
            for id in &ids[first_at.min(last_at)..=first_at.max(last_at)] {
                if !between.contains(id) {
                    between.push(id);
                }
            }
            let card = |album_id| crate::album::AlbumCard { album_id, section_id: None };
            let named = views
                .album_ids_between(&sort, group_by_artist, card(first_id), card(last_id))
                .await
                .unwrap();
            assert_eq!(named, between, "{what} between");

            // Chosen albums come back in the grid's order.
            let chosen = vec![last_id.to_string(), first_id.to_string()];
            let listed: Vec<String> = views
                .albums_in_browse_order(&sort, chosen.clone(), group_by_artist)
                .await
                .unwrap()
                .into_iter()
                .map(|album| album.id)
                .collect();
            let mut in_order = chosen;
            in_order.sort_by_key(|id| ids.iter().position(|listed| listed == id));
            in_order.dedup();
            assert_eq!(listed, in_order, "{what} chosen");

            if !group_by_artist {
                let page: Vec<String> = views
                    .get_album_page(&sort, 4, 5)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|album| album.id)
                    .collect();
                assert_eq!(page, ids[4..9], "{what} page");
            }
        }
    }
}
