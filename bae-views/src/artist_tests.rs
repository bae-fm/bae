//! The artist views: the artist list, its search and sorts, an artist's
//! detail, and the list's live query.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::{test_records, ARTIST_A, ARTIST_B, ARTIST_C, ARTIST_PRIMARY};
use bae_store::test_fixtures::live_query::*;
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const ALBUM_1999: &str = "88f57246-3e65-4eb9-8d36-ee8d40326cfc";
const ALBUM_2001_LOWER: &str = "88183677-683b-485e-8224-f6a328c233c7";
const ALBUM_2001_UPPER: &str = "a663cff7-fad7-45b1-8469-5f77af82ddb8";
const ALBUM_JUNCTION: &str = "7e6f42e7-8952-48e6-89bf-d1bcc611176d";
const ALBUM_NULL: &str = "c6648d5a-617e-4b69-87da-b7f1c4fb5e65";
const ARTIST_ABSENT: &str = "78420eae-1cd1-4a36-87ae-2a5556aa52aa";
const ARTIST_EXTRA: &str = "7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8";
const ARTIST_SOLO: &str = "49549823-0e72-4747-891e-ee50e1611e3a";
const ARTIST_VARIOUS: &str = "f862abf2-3b15-4518-889b-1996d7100201";
const ARTIST_WORK_ONLY: &str = "b96d8066-777d-408d-8ae4-ed58c767e40c";

/// Artists covering every membership case: a primary-FK artist that is
/// also a junction artist elsewhere, a junction-only artist, the Various
/// Artists row as a compilation's primary, a work-only composer (no album
/// links), and a fully unlinked artist.
async fn seeded_db() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let album_a = "a67c03ad-425f-45e9-8279-0144c852aaa5";
    let album_b = "a0231b0b-549b-4e4d-806f-a4b66373e087";
    let compilation = "20022731-6ca1-4bf3-8d27-1e2b2e5e9816";
    let work = "432c8996-8af0-43dc-868a-822a256f65c4";
    RecordsFixture::new()
        .artist(ARTIST_PRIMARY, "Artist Name B")
        .artist(ARTIST_EXTRA, "Artist Name A")
        .artist(ARTIST_VARIOUS, "Various Artists")
        .catalog_ids(ARTIST_VARIOUS, Some("194"), None)
        .artist(ARTIST_WORK_ONLY, "Composer Name A")
        .catalog_ids(ARTIST_WORK_ONLY, None, Some("mb-artist-work-only"))
        .artist("7d8362d9-b321-495a-89f7-4cd8998449a4", "Artist Name C")
        // The primary artist credited twice on album A must not count it
        // twice.
        .album(album_a, "Album Title A", Some(2001), &[ARTIST_PRIMARY, ARTIST_PRIMARY])
        .album(album_b, "Album Title B", Some(1999), &[ARTIST_PRIMARY, ARTIST_EXTRA])
        .album(compilation, "Compilation Title A", Some(2005), &[ARTIST_VARIOUS])
        .compilation(compilation)
        .release(album_a, "0252dedb-ee39-4547-8803-438dbeb57a64")
        .release(album_b, "64e79a1f-404a-4c34-809a-a3cb44bf1942")
        .release(compilation, "77f4af5a-9661-4fb7-845b-73901b0a3ebd")
        .work(work, "Work Title A")
        .composer(work, ARTIST_WORK_ONLY)
        .write(&test_records(&db))
        .await;
    (db, tmp)
}

/// The first ten artists `text` finds.
async fn search(db: &Store, text: &str) -> Vec<DbArtist> {
    LibraryViews::new(db.clone())
        .search_artists(&crate::LibrarySearchQuery::parse(text).expect("the text is not blank"), 10)
        .await
        .unwrap()
}

#[tokio::test]
async fn artist_search_ranks_exact_prefix_and_substring_matches() {
    let (db, _tmp) = seeded_db().await;
    let exact = "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b";
    RecordsFixture::new()
        .artist(exact, "Artist Search")
        .catalog_ids(exact, Some("discogs-exact"), Some("mb-exact"))
        .artist("6b312597-2c63-454e-9341-065704bd5f9f", "Artist Search Alpha")
        .artist("ef0f40c5-83b9-4e14-be41-210438e73ef1", "Artist Search Beta")
        .artist("d5cfb57f-0df4-4c99-9a1a-a634161c2e2c", "Name With Artist Search Inside")
        .sorted_artist(
            "1dd8239d-493b-434a-b0dd-d2838a9b404a",
            "Displayed Name",
            "Artist Search Sort",
        )
        .write(&test_records(&db))
        .await;

    let matches = search(&db, "artist search").await;
    let ids = matches.iter().map(|artist| artist.id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b",
            "6b312597-2c63-454e-9341-065704bd5f9f",
            "ef0f40c5-83b9-4e14-be41-210438e73ef1",
            "1dd8239d-493b-434a-b0dd-d2838a9b404a",
            "d5cfb57f-0df4-4c99-9a1a-a634161c2e2c",
        ]
    );
    assert_eq!(matches[0].discogs_artist_id.as_deref(), Some("discogs-exact"));
    assert_eq!(matches[0].musicbrainz_artist_id.as_deref(), Some("mb-exact"));

    let by_musicbrainz_id = search(&db, "mb-exact").await;
    assert_eq!(by_musicbrainz_id.len(), 1);
    assert_eq!(by_musicbrainz_id[0].id, "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b");

    let by_library_id = search(&db, "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b").await;
    assert_eq!(by_library_id.len(), 1);
    assert_eq!(by_library_id[0].discogs_artist_id.as_deref(), Some("discogs-exact"));
}

#[tokio::test]
async fn artist_page_lists_album_artists_with_distinct_album_counts() {
    let (db, _tmp) = seeded_db().await;

    let sort =
        [ArtistSortCriterion { field: ArtistSortField::Name, direction: SortDirection::Ascending }];
    let page = LibraryViews::new(db.clone()).get_artist_page(&sort, 0, 10).await.unwrap();

    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
    assert_eq!(ids, vec![ARTIST_EXTRA, ARTIST_PRIMARY, ARTIST_VARIOUS]);

    let counts: Vec<i64> = page.iter().map(|a| a.album_count).collect();
    assert_eq!(counts, vec![1, 2, 1]);

    assert_eq!(LibraryViews::new(db.clone()).get_artist_count().await.unwrap(), 3);
}

#[tokio::test]
async fn artist_page_sorts_by_album_count() {
    let (db, _tmp) = seeded_db().await;

    let sort = [ArtistSortCriterion {
        field: ArtistSortField::AlbumCount,
        direction: SortDirection::Descending,
    }];
    let page = LibraryViews::new(db.clone()).get_artist_page(&sort, 0, 10).await.unwrap();

    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
    assert_eq!(ids, vec![ARTIST_PRIMARY, ARTIST_EXTRA, ARTIST_VARIOUS]);
}

#[tokio::test]
async fn artist_page_uses_id_tiebreaker() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    // Three artists sharing a name, told apart by their MusicBrainz ids, each
    // credited on one album.
    let mut library = RecordsFixture::new();
    for (artist, album, title, release) in [
        (
            ARTIST_C,
            "881dc5cf-0686-456a-87c9-98c50e775177",
            "Album Title C",
            "599bc437-136f-4643-87a6-ac30c3fae614",
        ),
        (
            ARTIST_A,
            "a67c03ad-425f-45e9-8279-0144c852aaa5",
            "Album Title A",
            "0252dedb-ee39-4547-8803-438dbeb57a64",
        ),
        (
            ARTIST_B,
            "a0231b0b-549b-4e4d-806f-a4b66373e087",
            "Album Title B",
            "64e79a1f-404a-4c34-809a-a3cb44bf1942",
        ),
    ] {
        library = library
            .artist(artist, "Artist Name Shared")
            .catalog_ids(artist, None, Some(&format!("mb-{artist}")))
            .album(album, title, Some(2026), &[artist])
            .release(album, release);
    }
    library.write(&test_records(&db)).await;

    let sort = [ArtistSortCriterion {
        field: ArtistSortField::AlbumCount,
        direction: SortDirection::Descending,
    }];
    let mut page_ids = Vec::new();
    for offset in 0..3 {
        let page = LibraryViews::new(db.clone()).get_artist_page(&sort, offset, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        page_ids.push(page[0].artist.id.clone());
    }

    // Equal album counts, so the id breaks the tie ascending. The ids are
    // UUIDs, so the expectation is those same three sorted — asserting the
    // rule rather than a hand-written order that depends on which UUID the
    // fixture happened to mint.
    let mut expected = vec![ARTIST_A, ARTIST_B, ARTIST_C];
    expected.sort();
    assert_eq!(page_ids, expected);
}

/// A secondary criterion applies before the name-ASC tail. Two artists tie on
/// `AlbumCount` but differ in name, so `[AlbumCount DESC, Name DESC]` must order
/// the tied pair by name descending — a single-criterion implementation would
/// fall through to the tail's `ar.name ASC` and order them the other way.
#[tokio::test]
async fn artist_page_applies_secondary_criterion() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut library = RecordsFixture::new()
        .artist(ARTIST_A, "Artist Name A")
        .artist(ARTIST_B, "Artist Name B")
        .artist(ARTIST_SOLO, "Artist Name Solo");
    for (artist, album, title, release) in [
        (
            ARTIST_A,
            "1ac5b125-782f-4dab-8669-417e804d02bb",
            "Album Title A1",
            "5611ca91-e045-490a-8c14-89c3181a92ab",
        ),
        (
            ARTIST_A,
            "f270285f-ef24-4daa-8058-0dd91571843e",
            "Album Title A2",
            "48c59983-901e-455e-8cb0-ac0011c08bb4",
        ),
        (
            ARTIST_B,
            "2d705181-42c1-47bd-822c-245d8db41d60",
            "Album Title B1",
            "7680c516-307b-4a2a-8aea-850f976e006e",
        ),
        (
            ARTIST_B,
            "191beebd-146b-431f-8a87-ed4be0db20b7",
            "Album Title B2",
            "51ca3994-c53d-45e4-8ad2-04ac3f4181c8",
        ),
        (
            ARTIST_SOLO,
            "831374ed-abd3-4b6d-84b1-15a9974ecadc",
            "Album Title Solo",
            "7ae8e6fc-98bd-46e2-82c0-4b913087deb1",
        ),
    ] {
        library = library.album(album, title, Some(2026), &[artist]).release(album, release);
    }
    library.write(&test_records(&db)).await;

    let sort = [
        ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        },
        ArtistSortCriterion { field: ArtistSortField::Name, direction: SortDirection::Descending },
    ];
    let page = LibraryViews::new(db.clone()).get_artist_page(&sort, 0, 10).await.unwrap();
    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();

    // artist-a and artist-b tie on album_count (2 each); the secondary
    // Name DESC criterion orders artist-b before artist-a.
    assert_eq!(ids, vec![ARTIST_B, ARTIST_A, ARTIST_SOLO]);
}

#[tokio::test]
async fn artist_detail_orders_albums_year_then_title_with_unknown_years_last() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let artist_b = "4d0b27b7-c953-47f5-8614-70ed973923dc";
    let mut library =
        RecordsFixture::new().artist(ARTIST_A, "Artist Name A").artist(artist_b, "Artist Name B");
    for (album, title, year, credits, release) in [
        (
            ALBUM_NULL,
            "Album Title Null",
            None,
            &[ARTIST_A][..],
            "f09739aa-4dde-4cbc-8daf-d77eb2f980ff",
        ),
        (
            ALBUM_2001_UPPER,
            "Album Title B",
            Some(2001),
            &[ARTIST_A],
            "f582ee48-93a9-4152-8a12-7a9b62f86c2a",
        ),
        (
            ALBUM_2001_LOWER,
            "album title a",
            Some(2001),
            &[ARTIST_A],
            "7361d9e4-6e06-4a57-84fa-c6042cb2fb78",
        ),
        (
            ALBUM_1999,
            "Album Title 1999",
            Some(1999),
            &[ARTIST_A],
            "0014a775-79c9-419f-849e-8de944a1ef04",
        ),
        // Artist A second on Artist B's album.
        (
            ALBUM_JUNCTION,
            "Album Title Junction",
            Some(2005),
            &[artist_b, ARTIST_A],
            "d981c4b7-e727-41f9-826e-31ec67066d8c",
        ),
        (
            "ebef769a-f2a3-4443-8c01-50921de47fbb",
            "Album Title Unrelated",
            Some(1990),
            &[artist_b],
            "4fad85ec-94ef-4de6-8cf8-79dc25389051",
        ),
    ] {
        library = library.album(album, title, year, credits).release(album, release);
    }
    library.write(&test_records(&db)).await;

    let detail = LibraryViews::new(db.clone()).find_artist_detail(ARTIST_A).await.unwrap();
    let detail = detail.expect("artist-a has album links and must resolve");

    assert_eq!(detail.artist.artist.id, ARTIST_A);
    assert_eq!(detail.artist.album_count, 5);

    let album_ids: Vec<&str> = detail.albums.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        album_ids,
        vec![ALBUM_1999, ALBUM_2001_LOWER, ALBUM_2001_UPPER, ALBUM_JUNCTION, ALBUM_NULL,]
    );
}

#[tokio::test]
async fn artist_detail_absent_or_album_less_artist_is_none() {
    let (db, _tmp) = seeded_db().await;

    assert!(LibraryViews::new(db.clone())
        .find_artist_detail(ARTIST_ABSENT)
        .await
        .unwrap()
        .is_none());
    assert!(LibraryViews::new(db.clone())
        .find_artist_detail(ARTIST_WORK_ONLY)
        .await
        .unwrap()
        .is_none());
}

/// The artist grid reads the artists its window holds, and follows the images
/// of those artists only: the count it reads beside is its own query.
#[tokio::test]
async fn artist_window_reads_its_rows_and_their_images() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let views = crate::LibraryViews::new(db.clone());
    let mut live = views.subscribe_artist_window(&[], top_window());
    let mut count = views.subscribe_artist_count();

    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(count.next().await.expect("the query stays open").unwrap(), 1);
    assert_eq!(initial.rows[0].artist.id, ARTIST_ID);
    assert!(initial.rows[0].image.is_none());

    let artist_image = write_artist_image(&records, ARTIST_ID, "artist image bytes").await;
    let imaged = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("an image on a listed artist wakes artist browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        imaged.rows[0].image.as_ref().map(|image| image.version.as_str()),
        Some(artist_image.as_str())
    );
}

/// Windows of the artist list sharing bounds, each its own live query, and the
/// list's count its own: a write that lands in one window's range reruns that
/// window and the count and no other window.
#[tokio::test]
async fn an_artist_write_reruns_the_window_it_lands_in_and_the_count_alone() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut library = RecordsFixture::new();
    for (at, name) in ["A", "C", "E", "G", "I"].into_iter().enumerate() {
        let (artist, album, release) = (artist_id(at), album_id(at), release_id(at));
        library = library
            .artist(&artist, &format!("Artist Name {name}"))
            .album(&album, &format!("Album {name}"), None, &[&artist])
            .release(&album, &release);
    }
    library.write(&test_records(&db)).await;
    let views = crate::LibraryViews::new(db.clone());
    let second = views.artist_key_at(&[], 1).await.unwrap().expect("a row there");
    let fourth = views.artist_key_at(&[], 3).await.unwrap().expect("a row there");
    let mut live: Vec<_> = windows_split_at(second, fourth)
        .into_iter()
        .map(|window| views.subscribe_artist_window(&[], window))
        .collect();
    let mut count = views.subscribe_artist_count();
    for query in &mut live {
        tokio::time::timeout(HANG_GUARD, query.next()).await.unwrap().unwrap().result.unwrap();
    }
    count.next().await.expect("the query stays open").unwrap();

    // "D" sorts between the second row and the fourth.
    let (artist, album) = (artist_id(5), album_id(5));
    RecordsFixture::new()
        .artist(&artist, "Artist Name D")
        .album(&album, "Album D", None, &[&artist])
        .release(&album, &release_id(5))
        .write(&test_records(&db))
        .await;
    let marked: Vec<bool> = live.iter().map(|query| query.is_marked_for_rerun()).collect();
    assert_eq!(marked, [false, true, false]);
    assert!(count.is_marked_for_rerun());
}

fn artist_id(at: usize) -> String {
    format!("5e6f7a8b-0c1d-4e2f-8a3b-4c5d6e7f80{at:02}")
}

fn album_id(at: usize) -> String {
    format!("6f7a8b9c-1d2e-4f3a-9b4c-5d6e7f8a90{at:02}")
}

fn release_id(at: usize) -> String {
    format!("7a8b9c0d-2e3f-4a4b-8c5d-6e7f8a9b00{at:02}")
}
