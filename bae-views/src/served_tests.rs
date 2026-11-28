//! What a server outside bae lists: every artist, an artist's albums, a list
//! of albums and a search, each read in one statement that pages in SQL.

use super::*;
use bae_records::test_fixtures::audio::store_audio;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;

/// The test id named `name`.
fn id(name: &str) -> String {
    bae_util::test_ids::test_uuid(&format!("served-{name}"))
}

/// Artist A credits "Album Title B" (1999), whose releases `b1` and `b2`
/// were added in that order, and "Album Title A" (2005) with artist B, whose
/// release `a1` was added last and holds "Track Title B" then "Track Title
/// A", each a whole FLAC file.
async fn served_library() -> (Store, bae_util::test_files::TestFolder) {
    let (db, temp) = temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    let (artist_a, artist_b) = (id("artist-a"), id("artist-b"));
    let (album_a, album_b) = (id("album-a"), id("album-b"));
    RecordsFixture::new()
        .artist(&artist_a, "Artist Name A")
        .artist(&artist_b, "Artist Name B")
        .album(&album_b, "Album Title B", Some(1999), &[&artist_a])
        .album(&album_a, "Album Title A", Some(2005), &[&artist_a, &artist_b])
        .release(&album_b, &id("b1"))
        .at("2026-02-01T00:00:00Z")
        .release(&album_b, &id("b2"))
        .at("2026-03-01T00:00:00Z")
        .release(&album_a, &id("a1"))
        .tracks(&id("a1"), &[(&id("track-b"), "Track Title B"), (&id("track-a"), "Track Title A")])
        .write(&records)
        .await;
    for track in ["track-b", "track-a"] {
        store_audio(&records, &temp, &id("a1"), &id(track), false).await;
    }
    (db, temp)
}

fn release_ids(releases: &[DbServedRelease]) -> Vec<String> {
    releases.iter().map(|release| release.id.clone()).collect()
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| id(name)).collect()
}

/// Every artist lists in name order, each counting the releases of the
/// albums that credit it.
#[tokio::test]
async fn every_artist_lists_with_its_release_count() {
    let (db, _temp) = served_library().await;

    let artists = LibraryViews::new(db).served_artists().await.unwrap();

    let read: Vec<_> = artists
        .iter()
        .map(|artist| (artist.artist.artist.name.as_str(), artist.release_count))
        .collect();
    assert_eq!(read, [("Artist Name A", 3), ("Artist Name B", 1)]);
}

/// An artist's albums list as its page orders them, by year, each album's
/// releases in its order, and an artist the list doesn't show reads none.
#[tokio::test]
async fn an_artist_lists_the_releases_of_its_albums_in_its_pages_order() {
    let (db, _temp) = served_library().await;
    let views = LibraryViews::new(db);

    let (artist, releases) = views.served_artist(&id("artist-a")).await.unwrap().unwrap();

    assert_eq!(artist.release_count, 3);
    assert_eq!(release_ids(&releases), ids(&["b1", "b2", "a1"]));
    assert!(views.served_artist(&id("artist-absent")).await.unwrap().is_none());
}

/// An album list runs in the order its type names and pages in SQL.
#[tokio::test]
async fn an_album_list_orders_and_pages_its_releases() {
    let (db, _temp) = served_library().await;
    let views = LibraryViews::new(db);
    let all = ListPage { offset: 0, limit: 10 };
    let list =
        async |order, page| release_ids(&views.served_album_list(order, page).await.unwrap());

    assert_eq!(list(ServedAlbumOrder::Title, all).await, ids(&["a1", "b1", "b2"]));
    assert_eq!(list(ServedAlbumOrder::Title, ListPage { offset: 1, limit: 1 }).await, ids(&["b1"]));
    assert_eq!(list(ServedAlbumOrder::Newest, all).await, ids(&["a1", "b2", "b1"]));
    assert_eq!(
        list(ServedAlbumOrder::Years { from: 1990, to: 2010 }, all).await,
        ids(&["b1", "b2", "a1"])
    );
    assert_eq!(
        list(ServedAlbumOrder::Years { from: 2010, to: 1990 }, all).await,
        ids(&["a1", "b1", "b2"])
    );
    assert_eq!(list(ServedAlbumOrder::Years { from: 2000, to: 2010 }, all).await, ids(&["a1"]));
    let mut random = list(ServedAlbumOrder::Random, all).await;
    random.sort();
    let mut every = ids(&["a1", "b1", "b2"]);
    every.sort();
    assert_eq!(random, every);
}

/// A release reads as an album: its album's title, credits and year, its
/// track count and length, and its time added.
#[tokio::test]
async fn a_listed_release_reads_what_its_album_shows() {
    let (db, _temp) = served_library().await;

    let releases = LibraryViews::new(db)
        .served_album_list(ServedAlbumOrder::Title, ListPage { offset: 0, limit: 1 })
        .await
        .unwrap();

    let [release] = releases.as_slice() else { panic!("one release: {releases:?}") };
    assert_eq!(release.album_title, "Album Title A");
    assert_eq!(release.year, Some(2005));
    assert_eq!(release.song_count, 2);
    let credited: Vec<_> = release.credits.iter().map(|credit| credit.artist.id.clone()).collect();
    assert_eq!(credited, ids(&["artist-a", "artist-b"]));
}

/// A search pages each kind by its own count and offset, and no query lists
/// the whole library in the same orders.
#[tokio::test]
async fn a_search_pages_each_kind_on_its_own() {
    let (db, _temp) = served_library().await;
    let views = LibraryViews::new(db);
    let page = |offset, limit| ListPage { offset, limit };

    let whole = views
        .served_search(ServedSearchRequest {
            query: None,
            artists: page(1, 5),
            albums: page(0, 2),
            songs: page(0, 1),
        })
        .await
        .unwrap();
    let artists: Vec<_> =
        whole.artists.iter().map(|artist| artist.artist.artist.id.clone()).collect();
    assert_eq!(artists, ids(&["artist-b"]));
    assert_eq!(release_ids(&whole.albums), ids(&["a1", "b1"]));
    let songs: Vec<_> =
        whole.songs.iter().map(|(song, release)| (song.id.clone(), release.id.clone())).collect();
    assert_eq!(songs, [(id("track-a"), id("a1"))]);

    let found = views
        .served_search(ServedSearchRequest {
            query: LibrarySearchQuery::parse("title b"),
            artists: page(0, 5),
            albums: page(0, u64::MAX),
            songs: page(0, 5),
        })
        .await
        .unwrap();
    assert!(found.artists.is_empty());
    assert_eq!(release_ids(&found.albums), ids(&["b1", "b2"]));
    let songs: Vec<_> = found.songs.iter().map(|(song, _release)| song.id.clone()).collect();
    assert_eq!(songs, ids(&["track-b"]));
}
