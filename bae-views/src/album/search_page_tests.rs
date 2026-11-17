//! These tests check the albums a search lists as a media browser pages
//! through them. The browser reads each page once at its offset, and a live
//! query counts the albums. Each test writes its library through the records
//! domain's own writes.

use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const SEARCHED_ARTIST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5021";

/// `search_album_page` returns a search's albums one page at a time, in title
/// order; the page past the last album is empty. The search's album count is
/// a live query of its own, which counts a new album the search finds. With
/// no query, the count is zero.
#[tokio::test]
async fn a_search_pages_and_counts_its_albums_in_sql() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut fixture = RecordsFixture::new().artist(SEARCHED_ARTIST, "Paged Artist");
    for (id, release, title) in [
        (
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5031",
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5061",
            "Window A",
        ),
        (
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5032",
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5062",
            "Window B",
        ),
        (
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5033",
            "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5063",
            "Window C",
        ),
    ] {
        fixture = fixture.album(id, title, Some(2026), &[SEARCHED_ARTIST]).release(id, release);
    }
    fixture.write(&test_records(&db)).await;
    let views = crate::LibraryViews::new(db.clone());
    let query = crate::LibrarySearchQuery::parse("Window").unwrap();
    let page = |offset, limit| {
        let views = views.clone();
        let query = query.clone();
        async move {
            views
                .search_album_page(&query, offset, limit)
                .await
                .unwrap()
                .into_iter()
                .map(|album| album.title)
                .collect::<Vec<_>>()
        }
    };

    assert_eq!(page(0, 2).await, ["Window A", "Window B"]);
    assert_eq!(page(2, 2).await, ["Window C"]);
    assert!(page(4, 2).await.is_empty());

    let mut count = views.subscribe_search_album_count(Some(query.clone()));
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
