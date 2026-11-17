//! These tests check the albums a search lists as a media browser pages
//! through them. The browser reads a window of them from an album's key on,
//! live. A live query counts the albums. Each test writes its library
//! through the records domain's own writes.

use super::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const SEARCHED_ARTIST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5021";
const WINDOW_A: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5031";
const WINDOW_B: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5032";
const WINDOW_C: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5033";

/// A library with three albums a search for "Window" finds, each with one
/// release.
async fn searched_library() -> (LibraryViews, bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut fixture = RecordsFixture::new().artist(SEARCHED_ARTIST, "Paged Artist");
    for (id, release, title) in [
        (WINDOW_A, "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5061", "Window A"),
        (WINDOW_B, "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5062", "Window B"),
        (WINDOW_C, "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5063", "Window C"),
    ] {
        fixture = fixture.album(id, title, Some(2026), &[SEARCHED_ARTIST]).release(id, release);
    }
    fixture.write(&test_records(&db)).await;
    (LibraryViews::new(db.clone()), db, tmp)
}

fn window_query() -> Option<crate::LibrarySearchQuery> {
    crate::LibrarySearchQuery::parse("Window")
}

/// Each album's title in one delivery of a window of the albums a search
/// lists.
fn titles_of(read: WindowRead<DbAlbumSearchResult>) -> Vec<String> {
    let crate::ListRows::SearchAlbums(rows) = resolve_search_album_window(read).rows else {
        panic!("a window of a search's albums holds albums");
    };
    rows.into_iter().map(|album| album.title).collect()
}

/// The key of the album at `position` of those the search lists.
async fn key_of(views: &LibraryViews, position: u64) -> OrderKey {
    views.search_album_key_at(window_query(), position).await.unwrap().expect("an album there")
}

/// A window holds its limit of the search's albums from an album's key on,
/// in title order, and a key past the last album is the last album's. With
/// no query, the search lists no album.
#[tokio::test]
async fn a_search_window_holds_its_albums_from_a_key() {
    let (views, _db, _tmp) = searched_library().await;

    let mut live = views.subscribe_search_album_window(window_query(), ListWindow::top(2));
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(titles_of(read), ["Window A", "Window B"]);

    let third = ListWindow { from: Some(key_of(&views, 2).await), limit: 2 };
    let mut live = views.subscribe_search_album_window(window_query(), third);
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(titles_of(read), ["Window C"]);

    assert_eq!(key_of(&views, 3).await, key_of(&views, 2).await);
    assert_eq!(views.search_album_key_at(None, 0).await.unwrap(), None);
    let mut none = views.subscribe_search_album_window(None, ListWindow::top(2));
    let read = none.next_inline().await.expect("the query stays open").result.unwrap();
    assert!(read.rows.is_empty());
    let mut position =
        views.subscribe_search_album_position(window_query(), key_of(&views, 1).await);
    assert_eq!(position.next_inline().await.expect("the query stays open").result.unwrap(), 1);
}

/// A window follows a rename of an album it holds, so its next read shows
/// the album's new name.
#[tokio::test]
async fn a_renamed_album_in_a_window_shows_its_new_name_on_the_windows_next_read() {
    let (views, db, _tmp) = searched_library().await;
    let mut live = views.subscribe_search_album_window(window_query(), ListWindow::top(2));
    live.next_inline().await.expect("the query stays open").result.unwrap();

    test_records(&db).rename_album_for_test(WINDOW_B, "Window B Renamed").await.unwrap();
    let read = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap().result;
    assert_eq!(titles_of(read.unwrap()), ["Window A", "Window B Renamed"]);
}

/// A search lists at most [`crate::SEARCH_RESULT_LIMIT`] albums: a window
/// near the limit ends at it.
#[tokio::test]
async fn a_search_lists_no_album_past_its_limit() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut fixture = RecordsFixture::new().artist(SEARCHED_ARTIST, "Paged Artist");
    let listed = crate::SEARCH_RESULT_LIMIT as u64;
    for at in 0..=listed {
        let id = format!("f3a0e3a4-8c3d-4e6b-9f0b-{:012}", at);
        let release = format!("f3a0e3a4-8c3d-4e6b-9f0c-{:012}", at);
        fixture = fixture
            .album(&id, &format!("Window {at:03}"), Some(2026), &[SEARCHED_ARTIST])
            .release(&id, &release);
    }
    fixture.write(&test_records(&db)).await;
    let views = LibraryViews::new(db.clone());

    let from = views
        .search_album_key_at(window_query(), listed - 2)
        .await
        .unwrap()
        .expect("an album there");
    let mut live = views
        .subscribe_search_album_window(window_query(), ListWindow { from: Some(from), limit: 10 });
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(
        titles_of(read),
        [format!("Window {:03}", listed - 2), format!("Window {:03}", listed - 1)]
    );
    let mut count = views.subscribe_search_album_count(window_query());
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), listed);
}

/// The search's album count is a live query of its own, which counts a new
/// album the search finds. With no query, the count is zero.
#[tokio::test]
async fn a_search_counts_its_albums_live() {
    let (views, db, _tmp) = searched_library().await;
    let mut count = views.subscribe_search_album_count(window_query());
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 3);
    let (added, added_release) =
        ("f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5034", "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5064");
    RecordsFixture::new()
        .album(added, "Window D", Some(2026), &[SEARCHED_ARTIST])
        .release(added, added_release)
        .write(&test_records(&db))
        .await;
    let counted =
        tokio::time::timeout(HANG_GUARD, count.next_inline()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 4);
    assert_eq!(views.subscribe_search_album_count(None).next_inline().await.unwrap().unwrap(), 0);
}
