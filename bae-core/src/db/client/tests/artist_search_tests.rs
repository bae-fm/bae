//! The artist picker's live search: one subscription moved between texts and
//! windows, and woken by writes to what it finds.

use super::super::*;
use super::{exec, exec_batch};
use crate::library::{ArtistSearchView, LibraryPageWindow, LibrarySearchQuery};
use std::time::Duration;

const FIRST: &str = "1a7c1f53-6d0e-4f43-9a52-0c3f3a9f0a01";
const SECOND: &str = "2b8d2a64-7e1f-4a54-8b63-1d4a4b0a1b02";
const THIRD: &str = "3c9e3b75-8f2a-4b65-9c74-2e5b5c1b2c03";
const OTHER: &str = "4daf4c86-9a3b-4c76-8d85-3f6c6d2c3d04";

async fn seeded() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db().await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES
             ('{FIRST}', 'Artist Name A', 'artist name a', '2026-01-01T00:00:00Z'),
             ('{SECOND}', 'Artist Name B', 'artist name b', '2026-01-01T00:00:00Z'),
             ('{THIRD}', 'Artist Name C', 'artist name c', '2026-01-01T00:00:00Z'),
             ('{OTHER}', 'Composer Name', 'composer name', '2026-01-01T00:00:00Z');"
        ),
    )
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

fn ids(
    window: &crate::library::LibraryBrowseWindow<super::super::artist_search::DbArtistSearchRow>,
) -> Vec<&str> {
    window
        .rows
        .iter()
        .map(|row| row.artist.id.as_str())
        .collect()
}

/// Each window answers with its slice of what the text finds, and the count is
/// of everything found; a new text on the same subscription reads its own, and
/// no text finds nothing while still answering every window.
#[tokio::test]
async fn artist_search_moves_between_texts_and_windows_on_one_subscription() {
    let (db, _tmp) = seeded().await;
    let mut live = db.subscribe_artist_search(view("artist name", &[window(0, 2), window(2, 2)]));
    let requests = live.requests();

    let found = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(found.total_count, 3);
    assert_eq!(found.windows.len(), 2);
    assert_eq!(ids(&found.windows[0]), vec![FIRST, SECOND]);
    assert_eq!(ids(&found.windows[1]), vec![THIRD]);

    requests.set(view("composer", &[window(0, 2)])).unwrap();
    let other = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(other.total_count, 1);
    assert_eq!(ids(&other.windows[0]), vec![OTHER]);

    requests.set(view("   ", &[window(0, 2)])).unwrap();
    let none = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(none.total_count, 0);
    assert_eq!(none.windows.len(), 1, "every window answers");
    assert!(none.windows[0].rows.is_empty());
}

/// A new artist the text finds, and another printed name of a found artist,
/// each wake the search with what it now finds.
#[tokio::test]
async fn artist_search_follows_writes_to_what_it_finds() {
    let (db, _tmp) = seeded().await;
    let mut live = db.subscribe_artist_search(view("artist name", &[window(0, 10)]));
    let first = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(first.total_count, 3);
    assert!(first
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .all(|row| row.credit_names.is_empty()));

    exec(
        &db,
        "INSERT INTO artists (id, name, name_key, created_at) \
         VALUES (?1, 'Artist Name D', 'artist name d', '2026-01-01T00:00:00Z')",
        &["5eb05d97-ab4c-4d87-9e96-4a7d7e3d4e05"],
    )
    .await;
    let added = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a new match wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(added.total_count, 4);

    db.add_artist_credit_name(FIRST, "Artist Alias")
        .await
        .unwrap();
    let named = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    db.add_artist_credit_name(FIRST, "Zed Alias").await.unwrap();
    db.add_artist_credit_name(FIRST, "Alpha Alias")
        .await
        .unwrap();
    let image = super::live_query_tests::write_artist_image(&db, FIRST, "artist image bytes").await;

    let found = db
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
        first
            .credit_names
            .iter()
            .map(|name| name.credit.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha Alias", "Zed Alias"]
    );
}
