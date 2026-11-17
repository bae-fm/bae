//! The artist picker's live search. Each window, the count and the position
//! of a key are separate live queries, and a write to an artist the search
//! finds reruns them.

use crate::LibrarySearchQuery;
use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::live_query::write_artist_image;
use bae_records::test_fixtures::{credit, test_records};
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const FIRST: &str = "1a7c1f53-6d0e-4f43-9a52-0c3f3a9f0a01";
const SECOND: &str = "2b8d2a64-7e1f-4a54-8b63-1d4a4b0a1b02";
const THIRD: &str = "3c9e3b75-8f2a-4b65-9c74-2e5b5c1b2c03";
const OTHER: &str = "4daf4c86-9a3b-4c76-8d85-3f6c6d2c3d04";

async fn seeded() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(FIRST, "Artist Name A")
        .artist(SECOND, "Artist Name B")
        .artist(THIRD, "Artist Name C")
        .artist(OTHER, "Composer Name")
        .write(&test_records(&db))
        .await;
    (db, tmp)
}

fn query(text: &str) -> Option<LibrarySearchQuery> {
    LibrarySearchQuery::parse(text)
}

/// A window of the first `limit` rows found.
fn first(limit: u64) -> ListWindow {
    ListWindow::top(limit)
}

fn ids(window: &WindowRead<DbArtistSearchRow>) -> Vec<&str> {
    window.rows.iter().map(|(row, _key)| row.artist.id.as_str()).collect()
}

/// The first window of what `text` finds, read once.
async fn found(db: &Store, text: &str) -> WindowRead<DbArtistSearchRow> {
    LibraryViews::new(db.clone())
        .subscribe_artist_search_window(query(text), first(10))
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
}

/// Each window answers with its slice of what the text finds, and the count
/// is a separate query that counts every artist the text finds. A different
/// text opens different queries. An empty text returns no rows and a count of
/// zero.
#[tokio::test]
async fn artist_search_windows_hold_their_slices_and_the_count_its_own() {
    let (db, _tmp) = seeded().await;
    let views = LibraryViews::new(db.clone());
    let mut top = views.subscribe_artist_search_window(query("artist name"), first(2));
    let top = top.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(ids(&top), vec![FIRST, SECOND]);
    let third = views.artist_search_key_at(query("artist name"), 2).await.unwrap().expect("a row");
    let below = views
        .subscribe_artist_search_window(
            query("artist name"),
            ListWindow { from: Some(third), limit: 2 },
        )
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(ids(&below), vec![THIRD]);
    let count = views
        .subscribe_artist_search_count(query("artist name"))
        .next_inline()
        .await
        .expect("the query stays open")
        .unwrap();
    assert_eq!(count, 3);

    assert_eq!(ids(&found(&db, "composer").await), vec![OTHER]);
    let none = found(&db, "   ").await;
    assert!(none.rows.is_empty());
    let count =
        views.subscribe_artist_search_count(query("   ")).next_inline().await.unwrap().unwrap();
    assert_eq!(count, 0);
}

/// The position of a key in what the text finds counts the artists before it,
/// and a key at an offset is the row there.
#[tokio::test]
async fn artist_search_positions_and_jumps_by_key() {
    let (db, _tmp) = seeded().await;
    let views = LibraryViews::new(db.clone());
    let second =
        views.artist_search_key_at(query("artist name"), 1).await.unwrap().expect("a row at 1");
    let position = views
        .subscribe_artist_search_position(query("artist name"), second.clone())
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(position, 1);
    let from_there = views
        .subscribe_artist_search_window(
            query("artist name"),
            ListWindow { from: Some(second), limit: 10 },
        )
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(ids(&from_there), vec![SECOND, THIRD]);
    let last = views.artist_search_key_at(query("artist name"), 99).await.unwrap();
    assert_eq!(
        last,
        views.artist_search_key_at(query("artist name"), 2).await.unwrap(),
        "a position past the end is the last row's key"
    );
    assert_eq!(views.artist_search_key_at(query("   "), 0).await.unwrap(), None);
}

/// A new artist appears in the window from the row before it, which holds
/// its sort values, and the count goes up by one. Coven reruns every window
/// of the search for the write, because the search's order depends on the
/// text and coven cannot limit the rerun to the rows up to a window's last
/// row; each window delivers only what changed.
#[tokio::test]
async fn a_new_artist_lands_in_the_window_its_range_holds_and_the_count_follows() {
    let (db, _tmp) = seeded().await;
    let views = LibraryViews::new(db.clone());
    let second =
        views.artist_search_key_at(query("artist name"), 1).await.unwrap().expect("a row there");
    let third =
        views.artist_search_key_at(query("artist name"), 2).await.unwrap().expect("a row there");
    // A, then B and C, then C.
    let mut windows: Vec<_> = [
        ListWindow::top(1),
        ListWindow { from: Some(second), limit: 2 },
        ListWindow { from: Some(third), limit: 50 },
    ]
    .into_iter()
    .map(|window| views.subscribe_artist_search_window(query("artist name"), window))
    .collect();
    let mut count = views.subscribe_artist_search_count(query("artist name"));
    for window in &mut windows {
        tokio::time::timeout(HANG_GUARD, window.next_inline())
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
    }
    count.next_inline().await.expect("the query stays open").unwrap();

    // "Artist Name BB" sorts between B and C.
    test_records(&db).find_or_create_artists(&[credit("Artist Name BB")]).await.unwrap();
    // The windows above and below hold what they held, so a rerun of either
    // delivers nothing new; the middle one delivers the artist.
    let middle = tokio::time::timeout(HANG_GUARD, windows[1].next_inline())
        .await
        .expect("coven reruns the window for a new artist the text finds")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        middle.rows.iter().map(|(row, _key)| row.artist.name.as_str()).collect::<Vec<_>>(),
        ["Artist Name B", "Artist Name BB"]
    );
    let counted =
        tokio::time::timeout(HANG_GUARD, count.next_inline()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 4);
}

/// A new artist the text finds, and another printed name of a found artist,
/// each wake the search with what it now finds.
#[tokio::test]
async fn artist_search_follows_writes_to_what_it_finds() {
    let (db, _tmp) = seeded().await;
    let records = test_records(&db);
    let views = LibraryViews::new(db.clone());
    let mut live = views.subscribe_artist_search_window(query("artist name"), first(10));
    let mut count = views.subscribe_artist_search_count(query("artist name"));
    let initial = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 3);
    assert!(initial.rows.iter().all(|(row, _key)| row.credit_names.is_empty()));

    records.find_or_create_artists(&[credit("Artist Name D")]).await.unwrap();
    let added = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a new match wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(added.rows.len(), 4);
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 4);

    records
        .add_artist_credit_name(FIRST, &bae_store::EnteredName::parse("Artist Alias").unwrap())
        .await
        .unwrap();
    let named = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a found artist's new name wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    let names = &named
        .rows
        .iter()
        .find(|(row, _key)| row.artist.id == FIRST)
        .expect("the artist is found")
        .0
        .credit_names;
    assert_eq!(names.len(), 1);
    assert_eq!(names[0].credit.name, "Artist Alias");
}

/// A found artist carries, in its own row, its image's version and its other
/// printed names in name order.
#[tokio::test]
async fn a_found_artist_reads_its_image_and_names_in_its_row() {
    let (db, _tmp) = seeded().await;
    let records = test_records(&db);
    records
        .add_artist_credit_name(FIRST, &bae_store::EnteredName::parse("Zed Alias").unwrap())
        .await
        .unwrap();
    records
        .add_artist_credit_name(FIRST, &bae_store::EnteredName::parse("Alpha Alias").unwrap())
        .await
        .unwrap();
    let image = write_artist_image(&records, FIRST, "artist image bytes").await;

    let found = found(&db, "artist name").await;
    let (first, _key) = found
        .rows
        .iter()
        .find(|(row, _key)| row.artist.id == FIRST)
        .expect("the search finds the artist");
    assert_eq!(first.image_version.as_deref(), Some(image.as_str()));
    assert_eq!(
        first.credit_names.iter().map(|name| name.credit.name.as_str()).collect::<Vec<_>>(),
        ["Alpha Alias", "Zed Alias"]
    );
}

/// The picker folds case as library search does, past ASCII: a lowercase
/// accented letter finds the name printing it in capitals, and an exact
/// match ranks first.
#[tokio::test]
async fn the_picker_folds_case_past_ascii() {
    let (db, _tmp) = seeded().await;
    RecordsFixture::new()
        .artist("6fc16ea8-bc5d-4e98-8fa7-5b8e8f4e5f06", "Émile Artist Name")
        .artist("7ad27fb9-cd6e-4fa9-9ab8-6c9f9a5f6a07", "ÉMILE")
        .write(&test_records(&db))
        .await;

    assert_eq!(
        ids(&found(&db, "émile").await),
        vec!["7ad27fb9-cd6e-4fa9-9ab8-6c9f9a5f6a07", "6fc16ea8-bc5d-4e98-8fa7-5b8e8f4e5f06"]
    );
}
