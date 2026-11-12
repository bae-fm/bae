//! The artist picker's live search: one subscription moved between texts and
//! windows, and woken by writes to what it finds.

use crate::*;
use crate::{ArtistSearchView, LibrarySearchQuery};
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::live_query::write_artist_image;
use bae_records::test_fixtures::{credit, test_records};
use bae_store::test_fixtures::temp_db;
use bae_store::LibraryPageWindow;
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

fn window(offset: u64, limit: u64) -> LibraryPageWindow {
    LibraryPageWindow { offset, limit }
}

fn view(text: &str, windows: &[LibraryPageWindow]) -> ArtistSearchView {
    ArtistSearchView {
        query: LibrarySearchQuery::parse(text),
        windows: windows.iter().cloned().collect(),
    }
}

fn ids(window: &bae_store::LibraryBrowseWindow<DbArtistSearchRow>) -> Vec<&str> {
    window.rows.iter().map(|row| row.artist.id.as_str()).collect()
}

/// Each window answers with its slice of what the text finds, and the count is
/// of everything found; a new text on the same subscription reads its own, and
/// no text finds nothing while still answering every window.
#[tokio::test]
async fn artist_search_moves_between_texts_and_windows_on_one_subscription() {
    let (db, _tmp) = seeded().await;
    let mut live = LibraryViews::new(db.clone())
        .subscribe_artist_search(view("artist name", &[window(0, 2), window(2, 2)]));
    let requests = live.requests();

    let found = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(found.total_count, 3);
    assert_eq!(found.windows.len(), 2);
    assert_eq!(ids(&found.windows[0]), vec![FIRST, SECOND]);
    assert_eq!(ids(&found.windows[1]), vec![THIRD]);

    requests.set(view("composer", &[window(0, 2)])).unwrap();
    let other = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(other.total_count, 1);
    assert_eq!(ids(&other.windows[0]), vec![OTHER]);

    requests.set(view("   ", &[window(0, 2)])).unwrap();
    let none = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(none.total_count, 0);
    assert_eq!(none.windows.len(), 1, "every window answers");
    assert!(none.windows[0].rows.is_empty());
}

/// A window whose offset is past what SQLite counts finds no rows, rather than
/// the list's first.
#[tokio::test]
async fn a_window_far_past_the_end_is_empty() {
    let (db, _tmp) = seeded().await;
    let mut live = LibraryViews::new(db.clone()).subscribe_artist_search(view(
        "artist name",
        &[window(1 << 63, 2), window(u64::MAX, u64::MAX)],
    ));

    let found = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(found.total_count, 3);
    assert!(found.windows.iter().all(|window| window.rows.is_empty()));
}

/// A new artist the text finds, and another printed name of a found artist,
/// each wake the search with what it now finds.
#[tokio::test]
async fn artist_search_follows_writes_to_what_it_finds() {
    let (db, _tmp) = seeded().await;
    let records = test_records(&db);
    let mut live = LibraryViews::new(db.clone())
        .subscribe_artist_search(view("artist name", &[window(0, 10)]));
    let first = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(first.total_count, 3);
    assert!(first
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .all(|row| row.credit_names.is_empty()));

    records.find_or_create_artists(&[credit("Artist Name D")]).await.unwrap();
    let added = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a new match wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(added.total_count, 4);

    records
        .add_artist_credit_name(FIRST, &bae_store::EnteredName::parse("Artist Alias").unwrap())
        .await
        .unwrap();
    let named = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a found artist's new name wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    let names = &named
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .find(|row| row.artist.id == FIRST)
        .expect("the artist is found")
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

    let found = LibraryViews::new(db.clone())
        .subscribe_artist_search(view("artist name", &[window(0, 10)]))
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    let first = found.windows[0]
        .rows
        .iter()
        .find(|row| row.artist.id == FIRST)
        .expect("the artist is found");
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

    let found = LibraryViews::new(db.clone())
        .subscribe_artist_search(view("émile", &[window(0, 10)]))
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        ids(&found.windows[0]),
        vec!["7ad27fb9-cd6e-4fa9-9ab8-6c9f9a5f6a07", "6fc16ea8-bc5d-4e98-8fa7-5b8e8f4e5f06"]
    );
}
