//! These tests check the albums a search lists as a media browser pages
//! through them. The browser reads each page of them from the album at the
//! page's first place on, live. A live query counts the albums. Each test
//! writes its library through the records domain's own writes.

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
async fn searched_library() -> (LibraryViews, bae_store::Store, bae_util::test_files::TestFolder) {
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

/// Each album's title in one delivery of a page of the albums a search
/// lists.
fn titles_of(rows: crate::ListRows) -> Vec<String> {
    let crate::ListRows::SearchAlbums(rows) = rows else {
        panic!("a page of a search's albums holds albums");
    };
    rows.into_iter().map(|album| album.title).collect()
}

/// The page of `limit` albums `query` lists from `offset`, pulled.
fn page(
    views: &LibraryViews,
    query: Option<crate::LibrarySearchQuery>,
    offset: u64,
    limit: u64,
) -> bae_store::live_query::LiveValues<crate::ListRows> {
    crate::test_pages::page_values(
        views.subscribe_search_album_page(query, bae_store::ListPage { offset, limit }),
    )
}

/// A page holds its limit of the search's albums from the album at its first
/// place, in title order, and none past the last album. With no query, the
/// search lists no album.
#[tokio::test]
async fn a_search_page_holds_its_albums_from_its_first_place() {
    let (views, _db, _tmp) = searched_library().await;

    let rows = crate::test_pages::next_page(&page(&views, window_query(), 0, 2)).await;
    assert_eq!(titles_of(rows), ["Window A", "Window B"]);
    let rows = crate::test_pages::next_page(&page(&views, window_query(), 2, 2)).await;
    assert_eq!(titles_of(rows), ["Window C"]);
    let rows = crate::test_pages::next_page(&page(&views, window_query(), 3, 2)).await;
    assert!(titles_of(rows).is_empty());
    let rows = crate::test_pages::next_page(&page(&views, None, 0, 2)).await;
    assert!(titles_of(rows).is_empty());
}

/// A page follows a rename of an album it holds, so its next read shows the
/// album's new name.
#[tokio::test]
async fn a_renamed_album_in_a_page_shows_its_new_name_on_the_pages_next_read() {
    let (views, db, _tmp) = searched_library().await;
    let live = page(&views, window_query(), 0, 2);
    crate::test_pages::next_page(&live).await;

    test_records(&db).rename_album_for_test(WINDOW_B, "Window B Renamed").await.unwrap();
    let rows = crate::test_pages::next_page(&live).await;
    assert_eq!(titles_of(rows), ["Window A", "Window B Renamed"]);
}

/// A search lists at most [`crate::SEARCH_RESULT_LIMIT`] albums: a page near
/// the limit ends at it.
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

    let rows = crate::test_pages::next_page(&page(&views, window_query(), listed - 2, 10)).await;
    assert_eq!(
        titles_of(rows),
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

/// A search reads its albums in title order from the listing's title index,
/// so it stops at its cap: neither the first albums library search lists nor
/// a page of them computes a title's key or sorts albums, at any level of
/// the statement.
#[tokio::test]
async fn a_search_reads_its_albums_in_order_from_the_title_index() {
    let (db, _tmp) = crate::tests::library().await;
    let order = search_album_order();
    let params = vec![
        (":pattern", rusqlite::types::Value::Text(search_pattern("album 1"))),
        (":cap", rusqlite::types::Value::Integer(LISTED as i64)),
    ];
    let page = order.from_place(2, search_album_statement);
    for (what, seek) in [("first albums", order.everything()), ("page", page)] {
        let statement = search_album_statement(&order, &seek);
        assert!(!statement.contains("bae_name_key"), "{what}: {statement}");
        let steps = crate::tests::plan(&db, statement, seek.clone(), params.clone()).await;
        assert!(
            steps
                .iter()
                .any(|step| step.detail.contains("USING COVERING INDEX idx_album_listing_title")),
            "{what}: {steps:?}"
        );
        assert!(
            !steps.iter().any(|step| step.detail.starts_with("USE TEMP B-TREE FOR")
                && step.detail.ends_with("ORDER BY")),
            "{what}: {steps:?}"
        );
    }
}
