//! The album views' live queries: the grid, its selection, an album's detail
//! and library search, each taking new requests on the same live query and
//! woken by the writes that change what it shows, its own or another device's.

use super::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::composed_work::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::*;
use bae_records::{DbArtist, DbRelease};
use bae_store::test_fixtures::live_query::*;
use bae_store::test_fixtures::*;
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

/// The grid's selection reads through one subscription: its ids move in
/// place, it answers with the selected ids that name no album in the library,
/// and deleting a selected album's one release wakes the read that shows it
/// gone.
#[tokio::test]
async fn album_selection_follows_its_ids_on_one_subscription() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_selection(BTreeSet::new());
    let requests = live.requests();
    assert!(live.next_inline().await.expect("the query stays open").result.unwrap().is_empty());

    requests
        .set([ALBUM_ID.to_string(), "missing-album".to_string()].into_iter().collect())
        .unwrap();
    let selected = live.next_inline().await.expect("the query stays open");
    assert_eq!(selected.request.len(), 2);
    assert_eq!(selected.result.unwrap(), vec!["missing-album"]);

    delete_release(&test_records(&db), RELEASE_ID).await;
    let deleted = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("deleting a selected album's release wakes the selection")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted, vec![ALBUM_ID, "missing-album"]);
}

/// The window of the albums' first `limit` rows.
fn top(limit: u64) -> ListWindow {
    ListWindow::top(limit)
}

#[tokio::test]
async fn album_window_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let views = crate::LibraryViews::new(db.clone());
    let mut live = views.subscribe_album_window(&[], false, top(50));
    let counts = views.subscribe_album_count(false).next_inline().await.unwrap().unwrap();
    assert_eq!((counts.albums, counts.rows), (1, 1));

    let initial = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.rows[0].0.album.title, "Album Title");
    assert_eq!(initial.rows[0].0.album.cover_version, None);

    let cover = bae_records::test_fixtures::write_cover(&records, RELEASE_ID, "cover bytes").await;

    let updated = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("cover write wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.rows[0].0.album.cover_version.as_deref(), Some(cover.as_str()));
}

/// A grid card shows its album's primary release's cover, so that is the one
/// cover the browse reads: another release of the album gaining a cover is
/// not a change to the grid.
#[tokio::test]
async fn album_window_reads_only_the_primary_releases_cover() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    records.insert_release(&DbRelease::new_test(ALBUM_ID, OTHER_RELEASE_ID)).await.unwrap();
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_window(&[], false, top(50));
    live.next_inline().await.expect("the query stays open").result.unwrap();

    bae_records::test_fixtures::write_cover(
        &records,
        OTHER_RELEASE_ID,
        "other release cover bytes",
    )
    .await;
    let primary = bae_records::test_fixtures::write_cover(
        &records,
        RELEASE_ID,
        "primary release cover bytes",
    )
    .await;
    let updated = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("the primary release's cover wakes the browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.rows[0].0.album.cover_version.as_deref(), Some(primary.as_str()));
}

/// A window's request changes on the same live query, each value naming the
/// window it answers; a write inside its range wakes it, one outside it does
/// not, and a row whose sort values move it out of the range leaves it.
#[tokio::test]
async fn album_window_subscription_moves_in_place_and_follows_its_range() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    RecordsFixture::new()
        .album(OTHER_ALBUM_ID, "Album Title Second", None, &[ARTIST_ID])
        .release(OTHER_ALBUM_ID, OTHER_RELEASE_ID)
        .write(&records)
        .await;
    let views = crate::LibraryViews::new(db.clone());
    let mut live = views.subscribe_album_window(&sort, false, top(50));
    let requests = live.requests();
    let initial_event = live.next_inline().await.expect("the query stays open");
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.window, top(50));
    assert_eq!(
        initial.rows.iter().map(|(row, _key)| row.album.id.as_str()).collect::<Vec<_>>(),
        [ALBUM_ID, OTHER_ALBUM_ID]
    );

    // The window from the second album on.
    let second = views.album_key_at(&sort, false, 1).await.unwrap().unwrap();
    let from_second = ListWindow { from: Some(second), limit: 50 };
    let revision = requests.set(from_second.clone()).unwrap();
    let event = live.next_inline().await.expect("the query stays open");
    assert_eq!((&event.request, event.revision), (&from_second, revision));
    let moved = event.result.unwrap();
    assert_eq!(moved.window, from_second);
    assert_eq!(moved.rows.len(), 1);
    assert_eq!(moved.rows[0].0.album.id, OTHER_ALBUM_ID);

    // An edit that keeps the album in the range wakes it.
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Title Second Renamed").await.unwrap();
    let renamed = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("an album in the window's range wakes it")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(renamed.rows[0].0.album.title, "Album Title Second Renamed");

    // A cover on an album in the range wakes it.
    let cover_in_range =
        bae_records::test_fixtures::write_cover(&records, OTHER_RELEASE_ID, "inside").await;
    let covered = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a cover of an album in the range wakes the window")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(covered.rows[0].0.album.cover_version.as_deref(), Some(cover_in_range.as_str()));

    // A title that sorts the album above the window's start takes it out of
    // the range: the window delivers none, and the album's edits after that
    // are outside it.
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Title").await.unwrap();
    let left = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("an album leaving the range wakes the window")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(left.rows.is_empty());
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Title Again").await.unwrap();
    assert!(
        !live.is_marked_for_rerun(),
        "an edit to an album that left the range does not rerun the window"
    );

    // The top of the list holds it again, first.
    requests.set(top(50)).unwrap();
    let top_again = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(top_again.rows[0].0.album.id, OTHER_ALBUM_ID);

    delete_release(&records, OTHER_RELEASE_ID).await;
    let deleted = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(deleted.rows.len(), 1);
    assert_eq!(views.subscribe_album_count(false).next_inline().await.unwrap().unwrap().rows, 1);
}

#[tokio::test]
async fn album_window_ignores_an_unread_table() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_window(&[], false, top(50));
    let initial = live.next_inline().await.expect("the query stays open").result.unwrap();

    bae_playback::tables::PlaybackQueueTables::new(db.clone())
        .save_playback_state(&bae_playback::tables::PlaybackResumeRow {
            current: None,
            position_ms: Some(0),
        })
        .await
        .unwrap();

    assert!(
        !live.is_marked_for_rerun(),
        "a write to a table the window does not read does not rerun the window"
    );
    let moved = next_run_is_only_the_request(&mut live, top(49)).await.result.unwrap();
    assert_eq!(moved.rows, initial.rows);
}

#[test]
fn album_page_subscription_delivers_a_write_materialized_by_sync() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let writer = devices.a();
        let reader = devices.b();
        let mut live =
            crate::LibraryViews::new(reader.clone()).subscribe_album_window(&[], false, top(50));
        assert!(live
            .next_inline()
            .await
            .expect("the query stays open")
            .result
            .unwrap()
            .rows
            .is_empty());
        RecordsFixture::new()
            .artist(ARTIST_ID, "Artist Name")
            .album(ALBUM_ID, "Synced Album", None, &[ARTIST_ID])
            .release(ALBUM_ID, RELEASE_ID)
            .write(&test_records(writer))
            .await;
        devices
            .settle("the remote album", async |_, db| {
                bae_records::test_fixtures::test_records(db)
                    .find_album_by_id(ALBUM_ID)
                    .await
                    .unwrap()
                    .is_some()
            })
            .await;
        let updated = tokio::time::timeout(HANG_GUARD, live.next_inline())
            .await
            .unwrap()
            .expect("the query stays open")
            .result
            .unwrap();
        assert_eq!(updated.rows[0].0.album.title, "Synced Album");
    });
}

/// B's search tables are local, written by B's triggers as it applies A's
/// writes: an album A writes, then renames, is found on B by each title in
/// turn.
#[test]
fn library_search_finds_rows_applied_from_another_device() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let writer = devices.a();
        let reader = devices.b();
        RecordsFixture::new()
            .artist(ARTIST_ID, "Artist Name")
            .album(ALBUM_ID, "Écrin Album Title", None, &[ARTIST_ID])
            .release(ALBUM_ID, RELEASE_ID)
            .write(&test_records(writer))
            .await;
        devices
            .settle("the remote album", async |_, db| {
                bae_records::test_fixtures::test_records(db)
                    .find_album_by_id(ALBUM_ID)
                    .await
                    .unwrap()
                    .is_some()
            })
            .await;
        let found =
            crate::LibraryViews::new(reader.clone()).search_rows("écrin", 10).await.unwrap();
        assert_eq!(
            found.albums.iter().map(|album| album.id.as_str()).collect::<Vec<_>>(),
            vec![ALBUM_ID]
        );

        test_records(writer).rename_album_for_test(ALBUM_ID, "Renamed Album Title").await.unwrap();
        devices
            .settle("the remote rename", async |_, db| {
                bae_records::test_fixtures::test_records(db)
                    .find_album_by_id(ALBUM_ID)
                    .await
                    .unwrap()
                    .unwrap()
                    .title
                    == "Renamed Album Title"
            })
            .await;
        assert!(crate::LibraryViews::new(reader.clone())
            .search_rows("écrin", 10)
            .await
            .unwrap()
            .albums
            .is_empty());
        assert_eq!(
            crate::LibraryViews::new(reader.clone())
                .search_rows("renamed album", 10)
                .await
                .unwrap()
                .albums
                .len(),
            1
        );
    });
}

#[tokio::test]
async fn album_summary_orders_artist_names_and_release_ids_inside_aggregates() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    // The one album in the fixture, read the way the library page reads it.
    let summary = crate::LibraryViews::new(db.clone())
        .get_album_page(&[], 0, 1)
        .await
        .unwrap()
        .pop()
        .expect("album summary row");

    assert_eq!(
        summary.artist_line.plain_text(),
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(summary.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

const ALBUM_PERCENT: &str = "7e9948c4-f2d0-4a73-8e5c-a885eda086ff";
const ALBUM_UNDERSCORE: &str = "2dd55a3f-3208-4faf-8737-453f474074cb";
const TRACK_PERCENT: &str = "4dc8cde9-15fb-470d-802c-b7e5f1ccc63d";
const TRACK_UNDERSCORE: &str = "b2930937-dae6-4719-8150-aa61422eeeac";

#[tokio::test]
async fn search_library_matches_album_artist_names() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let results =
        crate::LibraryViews::new(db.clone()).search_rows("Album Artist A", 10).await.unwrap();
    assert_eq!(results.albums.len(), 1);
    assert_eq!(results.albums[0].id, ALBUM_A);
}

#[tokio::test]
async fn search_library_returns_artist_hits() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let results =
        crate::LibraryViews::new(db.clone()).search_rows("Album Artist A", 10).await.unwrap();
    assert_eq!(results.artists.len(), 1);
    assert_eq!(results.artists[0].artist.id, ARTIST_ALBUM);
    assert_eq!(results.artists[0].album_count, 1);

    // Composer artists are artist rows too, but only surface as artist
    // hits when they have albums; the seeded composer has none.
    let composer =
        crate::LibraryViews::new(db.clone()).search_rows("Displayed Composer A", 10).await.unwrap();
    assert_eq!(composer.composers.len(), 1);
    assert!(composer.artists.is_empty());
}

/// Library search finds an artist, and a composer, by any other name its
/// credits print, as it finds their albums.
#[tokio::test]
async fn search_library_finds_an_artist_by_another_printed_name() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    for (artist, name) in [(ARTIST_ALBUM, "Printed Alias"), (ARTIST_COMPOSER, "Composer Alias")] {
        records
            .add_artist_credit_name(artist, &bae_store::EnteredName::parse(name).unwrap())
            .await
            .unwrap();
    }

    let artist =
        crate::LibraryViews::new(db.clone()).search_rows("printed alias", 10).await.unwrap();
    assert_eq!(
        artist.artists.iter().map(|found| found.artist.id.as_str()).collect::<Vec<_>>(),
        [ARTIST_ALBUM]
    );
    let composer =
        crate::LibraryViews::new(db.clone()).search_rows("composer alias", 10).await.unwrap();
    assert_eq!(
        composer.composers.iter().map(|found| found.artist.id.as_str()).collect::<Vec<_>>(),
        [ARTIST_COMPOSER]
    );
}

#[tokio::test]
async fn search_library_matches_composer_and_work_sort_names() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let composer_results =
        crate::LibraryViews::new(db.clone()).search_rows("Hidden Composer", 10).await.unwrap();
    assert_eq!(composer_results.composers.len(), 1);
    assert_eq!(composer_results.composers[0].artist.id, ARTIST_COMPOSER);

    write_cover(&test_records(&db), RELEASE_A, "cover bytes").await;
    let work_results =
        crate::LibraryViews::new(db.clone()).search_rows("Displayed Work", 10).await.unwrap();
    assert_eq!(work_results.works.len(), 1);
    assert_eq!(work_results.works[0].work.id, WORK_CHILD_A);
    assert_eq!(
        work_results.works[0].representative_cover.as_ref().map(|cover| cover.id.as_str()),
        Some(RELEASE_A)
    );
}

#[tokio::test]
async fn search_library_treats_like_metacharacters_as_literals() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let mut library = RecordsFixture::new().artist(ARTIST_PRIMARY, "Artist Name Primary");
    for (album, title, release, track, track_title) in [
        (
            ALBUM_PERCENT,
            "50% Album Title",
            "94d062dd-9dc8-4e59-8c13-e5731b702157",
            TRACK_PERCENT,
            "50% Track Title",
        ),
        (
            "531c76e6-3ed8-45cd-8ee8-63366ef42031",
            "500 Album Title",
            "7a4de476-4ce8-4e2f-805a-d4c23dffaf8e",
            "37610729-bd6c-4511-81a9-8848a832ac73",
            "500 Track Title",
        ),
        (
            ALBUM_UNDERSCORE,
            "A_B Album Title",
            "07d51116-f6e3-4b4c-8ae2-601d5a972bf9",
            TRACK_UNDERSCORE,
            "A_B Track Title",
        ),
        (
            "e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc",
            "ACB Album Title",
            "3dc45e4f-fe7f-47aa-84fc-4d6027c57276",
            "1f084b57-7c9c-457f-80fb-c61688c31175",
            "ACB Track Title",
        ),
    ] {
        library = library
            .album(album, title, Some(2026), &[ARTIST_PRIMARY])
            .release(album, release)
            .tracks(release, &[(track, track_title)]);
    }
    library.write(&test_records(&db)).await;

    let percent_results =
        crate::LibraryViews::new(db.clone()).search_rows("50%", 10).await.unwrap();
    assert_eq!(percent_results.albums.len(), 1);
    assert_eq!(percent_results.albums[0].id, ALBUM_PERCENT);
    assert_eq!(percent_results.tracks.len(), 1);
    assert_eq!(percent_results.tracks[0].id, TRACK_PERCENT);

    let underscore_results =
        crate::LibraryViews::new(db.clone()).search_rows("A_B", 10).await.unwrap();
    assert_eq!(underscore_results.albums.len(), 1);
    assert_eq!(underscore_results.albums[0].id, ALBUM_UNDERSCORE);
    assert_eq!(underscore_results.tracks.len(), 1);
    assert_eq!(underscore_results.tracks[0].id, TRACK_UNDERSCORE);
}

const NEEDLE_ARTIST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a1111";
const ALBUM_FOO: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a2001";
const RELEASE_FOO: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a2002";
const TRACK_FOO_FIRST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a2003";
const TRACK_FOO_SECOND: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a2004";
const ALBUM_NEEDLE_FIRST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a3001";
const RELEASE_NEEDLE_FIRST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a3002";
const TRACK_NEEDLE_FIRST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a3003";
const ALBUM_NEEDLE_LAST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a4001";
const RELEASE_NEEDLE_LAST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a4002";
const TRACK_NEEDLE_SONG: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a4003";

/// `search_top_playable` returns the track that a spoken "play" of a search
/// plays: the first track found, else the first track of the first album
/// found, and nothing when the search finds neither, as the library's SQL
/// orders them.
#[tokio::test]
async fn a_search_names_the_track_a_spoken_play_plays() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(NEEDLE_ARTIST, "Needle Artist Name")
        .album(ALBUM_FOO, "Foo Album Title", Some(2026), &[NEEDLE_ARTIST])
        .release(ALBUM_FOO, RELEASE_FOO)
        .tracks(
            RELEASE_FOO,
            &[(TRACK_FOO_FIRST, "Opening Title"), (TRACK_FOO_SECOND, "Second Title")],
        )
        .album(ALBUM_NEEDLE_FIRST, "Needle Aardvark Album", Some(2026), &[NEEDLE_ARTIST])
        .release(ALBUM_NEEDLE_FIRST, RELEASE_NEEDLE_FIRST)
        .tracks(RELEASE_NEEDLE_FIRST, &[(TRACK_NEEDLE_FIRST, "Zzz Title")])
        .album(ALBUM_NEEDLE_LAST, "Zed Needle Album", Some(2026), &[NEEDLE_ARTIST])
        .release(ALBUM_NEEDLE_LAST, RELEASE_NEEDLE_LAST)
        .tracks(RELEASE_NEEDLE_LAST, &[(TRACK_NEEDLE_SONG, "Needle Song")])
        .write(&test_records(&db))
        .await;
    let top = |query: &'static str| {
        let views = crate::LibraryViews::new(db.clone());
        async move {
            let query = crate::LibrarySearchQuery::parse(query).unwrap();
            let top = views.search_top_playable(&query).await.unwrap();
            top.map(|top| (top.release_id, top.track_id))
        }
    };

    assert_eq!(
        top("Second Title").await,
        Some((RELEASE_FOO.to_string(), TRACK_FOO_SECOND.to_string())),
        "a track found is the track to play"
    );
    assert_eq!(
        top("Foo Album").await,
        Some((RELEASE_FOO.to_string(), TRACK_FOO_FIRST.to_string())),
        "an album found plays its main release's first track"
    );
    assert_eq!(
        top("Needle").await,
        Some((RELEASE_NEEDLE_LAST.to_string(), TRACK_NEEDLE_SONG.to_string())),
        "a track found outranks the albums found, whatever their order"
    );
    assert_eq!(top("Nothing Matches This").await, None);
}

/// A search says whether it found anything of any kind, so no app screen
/// tests the album, track, artist, composer and work lists to show that
/// nothing matched: a track title, an album title and an artist's name each
/// count, and a query that matches none does not. A search also counts the
/// rows in each section.
#[tokio::test]
async fn a_search_says_whether_it_found_anything() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(NEEDLE_ARTIST, "Needle Artist Name")
        .album(ALBUM_FOO, "Foo Album Title", Some(2026), &[NEEDLE_ARTIST])
        .release(ALBUM_FOO, RELEASE_FOO)
        .tracks(RELEASE_FOO, &[(TRACK_FOO_FIRST, "Opening Title")])
        .write(&test_records(&db))
        .await;
    let found = |query: &'static str| {
        let views = crate::LibraryViews::new(db.clone());
        async move { views.search_rows(query, 10).await.unwrap().found }
    };

    assert!(found("Opening Title").await, "a track found");
    assert!(found("Foo Album").await, "an album found");
    assert!(found("Needle Artist").await, "an artist found");
    assert!(!found("Nothing Matches This").await);

    let views = crate::LibraryViews::new(db.clone());
    let counts = views.search_rows("Title", 10).await.unwrap().section_counts;
    assert_eq!(
        counts,
        crate::SearchSectionCounts { albums: 1, artists: 0, tracks: 1, composers: 0, works: 0 },
        "each section counts the rows it lists"
    );
    let capped = views.search_rows("Title", 0).await.unwrap().section_counts;
    assert_eq!(capped.albums, 0, "a section counts no more rows than it lists");
}

const EMPTY_ALBUM: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5001";
const EMPTY_RELEASE: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5002";
const PLAYABLE_ALBUM: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5011";
const PLAYABLE_RELEASE: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5012";
const PLAYABLE_TRACK: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5013";

/// An album found whose main release has no track plays nothing, so the spoken
/// "play" takes the next album found that has one.
#[tokio::test]
async fn a_search_skips_a_found_album_with_no_track_to_play() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(NEEDLE_ARTIST, "Needle Artist Name")
        .album(EMPTY_ALBUM, "Alpha Probe", Some(2026), &[NEEDLE_ARTIST])
        .release(EMPTY_ALBUM, EMPTY_RELEASE)
        .album(PLAYABLE_ALBUM, "Beta Probe", Some(2026), &[NEEDLE_ARTIST])
        .release(PLAYABLE_ALBUM, PLAYABLE_RELEASE)
        .tracks(PLAYABLE_RELEASE, &[(PLAYABLE_TRACK, "Only Title")])
        .write(&test_records(&db))
        .await;

    let query = crate::LibrarySearchQuery::parse("Probe").unwrap();
    let top = crate::LibraryViews::new(db.clone())
        .search_top_playable(&query)
        .await
        .unwrap()
        .map(|top| (top.release_id, top.track_id));

    assert_eq!(top, Some((PLAYABLE_RELEASE.to_string(), PLAYABLE_TRACK.to_string())));
}

const PAGED_ARTIST: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5021";

const PICKED_TRACK: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5043";
const OTHER_TRACK: &str = "f3a0e3a4-8c3d-4e6b-9f0b-4a6f3f6a5044";

/// `find_release_track` returns the browse item for a track by its release
/// and track ids, and none when the track is on another release.
#[tokio::test]
async fn a_release_track_is_read_by_its_ids() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(PAGED_ARTIST, "Paged Artist")
        .album(PLAYABLE_ALBUM, "Picked Album", Some(2026), &[PAGED_ARTIST])
        .release(PLAYABLE_ALBUM, PLAYABLE_RELEASE)
        .tracks(PLAYABLE_RELEASE, &[(PICKED_TRACK, "Picked"), (OTHER_TRACK, "Other")])
        .write(&test_records(&db))
        .await;
    let views = crate::LibraryViews::new(db.clone());

    let picked = views.find_release_track(PLAYABLE_RELEASE, PICKED_TRACK).await.unwrap();
    assert_eq!(
        picked.map(|track| (track.id, track.title, track.release_id)),
        Some((PICKED_TRACK.to_string(), "Picked".to_string(), PLAYABLE_RELEASE.to_string(),))
    );
    assert!(views.find_release_track(EMPTY_RELEASE, PICKED_TRACK).await.unwrap().is_none());
}

const SEARCHED_ARTIST: &str = "0373bb67-3abb-48e7-8a5b-6d29ef24308e";
const SEARCHED_MUSICBRAINZ_ARTIST: &str = "mb-artist-asel";
const SEARCHED_ALBUM: &str = "e307f0f5-e518-4d5e-bea8-83e439780ca0";
const SEARCHED_TRACK: &str = "b2140894-e938-4143-9c9a-5e054c2d7dde";
const SEARCHED_WORK: &str = "cef1d7ca-5e1b-4144-9e95-3ece6a68725e";
const SEARCHED_RELEASE: &str = "45397b28-ee0e-4e87-921e-4fd7f9f07478";

/// One album by `artist`, on one release of one track performing one work
/// the artist wrote; every title and name starts with a letter outside ASCII.
fn searched_library(artist: &str) -> RecordsFixture {
    RecordsFixture::new()
        .album(SEARCHED_ALBUM, "Écrin Album Title", Some(2026), &[artist])
        .release(SEARCHED_ALBUM, SEARCHED_RELEASE)
        .tracks(SEARCHED_RELEASE, &[(SEARCHED_TRACK, "Übel Track Title")])
        .work(SEARCHED_WORK, "Œvra Work Title")
        .composer(SEARCHED_WORK, artist)
        .performs(SEARCHED_TRACK, SEARCHED_WORK)
}

/// [`searched_library`] by the artist `SEARCHED_ARTIST`, named "Åsel Artist
/// Name", on `db`.
async fn write_searched_library(db: &Store) {
    RecordsFixture::new()
        .artist(SEARCHED_ARTIST, "Åsel Artist Name")
        .write(&test_records(db))
        .await;
    searched_library(SEARCHED_ARTIST).write(&test_records(db)).await;
}

/// What a search for `query` finds: the ids of its albums, tracks, artists,
/// composers and works.
async fn found(db: &Store, query: &str) -> [Vec<String>; 5] {
    let results = crate::LibraryViews::new(db.clone()).search_rows(query, 10).await.unwrap();
    [
        results.albums.into_iter().map(|album| album.id).collect(),
        results.tracks.into_iter().map(|track| track.id).collect(),
        results.artists.into_iter().map(|artist| artist.artist.id).collect(),
        results.composers.into_iter().map(|composer| composer.artist.id).collect(),
        results.works.into_iter().map(|work| work.work.id).collect(),
    ]
}

#[tokio::test]
async fn search_library_ignores_case_outside_ascii() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_searched_library(&db).await;

    let ids = |id: &str| vec![id.to_string()];
    assert_eq!(found(&db, "écrin").await[0], ids(SEARCHED_ALBUM));
    assert_eq!(found(&db, "ÉCRIN").await[0], ids(SEARCHED_ALBUM));
    assert_eq!(found(&db, "übel").await[1], ids(SEARCHED_TRACK));
    let artist = found(&db, "åsel").await;
    assert_eq!(artist[0], ids(SEARCHED_ALBUM), "the album by its artist's name");
    assert_eq!(artist[2], ids(SEARCHED_ARTIST));
    assert_eq!(artist[3], ids(SEARCHED_ARTIST));
    assert_eq!(found(&db, "œvra").await[4], ids(SEARCHED_WORK));
    // Folding is case only: an unaccented query doesn't find accented text.
    assert!(found(&db, "ecrin").await[0].is_empty());
}

#[tokio::test]
async fn search_library_reads_wildcards_as_themselves() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_searched_library(&db).await;
    test_records(&db).rename_album_for_test(SEARCHED_ALBUM, "Écrin*[x]?Album").await.unwrap();

    assert_eq!(found(&db, "n*[x]?a").await[0], vec![SEARCHED_ALBUM.to_string()]);
    assert!(found(&db, "nx").await[0].is_empty());
    assert!(found(&db, "n?[").await[0].is_empty());
    assert!(found(&db, "*album").await[0].is_empty());
}

/// A credit printing another of the artist's names finds the album by it.
#[tokio::test]
async fn search_library_follows_the_names_credits_print() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_searched_library(&db).await;

    test_records(&db)
        .save_release_metadata_for_test(SEARCHED_RELEASE, |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Printed Näme"));
        })
        .await
        .unwrap();
    assert_eq!(found(&db, "printed näme").await[0], vec![SEARCHED_ALBUM.to_string()]);

    // Deleting the album's one release takes its track, the album and the work
    // no track performs, and search finds none of them.
    delete_release(&test_records(&db), SEARCHED_RELEASE).await;
    let after = found(&db, "title").await;
    assert!(after[0].is_empty() && after[1].is_empty());
    assert!(after[4].is_empty());
}

/// Two devices apart each name one MusicBrainz artist, so one artist row,
/// differently; once both apply both writes the later name holds, and search
/// on the device whose name lost finds the album by the other.
#[test]
fn search_library_follows_an_artist_named_otherwise_on_another_device() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let artist = bae_records::identity::artist_id(Some(SEARCHED_MUSICBRAINZ_ARTIST), None)
            .expect("a catalog artist's id");
        let named = |name: &str| DbArtist {
            musicbrainz_artist_id: Some(SEARCHED_MUSICBRAINZ_ARTIST.to_string()),
            ..credit(name)
        };
        devices.pause().await;
        let first = test_records(devices.a());
        assert_eq!(
            first.find_or_create_artists(&[named("Åsel Artist Name")]).await.unwrap(),
            [artist.as_str()]
        );
        searched_library(&artist).write(&first).await;
        assert_eq!(found(devices.a(), "åsel").await[0], vec![SEARCHED_ALBUM.to_string()]);
        let second = test_records(devices.b());
        assert_eq!(
            second.find_or_create_artists(&[named("Renamed Artist Name")]).await.unwrap(),
            [artist.as_str()]
        );
        devices.resume().await;

        devices
            .settle("the later name", async |_, db| {
                test_records(db).find_artist_by_id(&artist).await.unwrap().map(|found| found.name)
                    == Some("Renamed Artist Name".to_string())
            })
            .await;
        assert!(found(devices.a(), "åsel").await[0].is_empty());
        assert_eq!(found(devices.a(), "renamed artist").await[0], vec![SEARCHED_ALBUM.to_string()]);
    });
}

const WORK_NO_ARTISTS: &str = "0f6b8b9c-5d0e-4c53-a2e0-2a1e5a6a3a41";

/// A track performing two works lists both in the order it performs them, a
/// work with no artists has no composer names, and a track performing none
/// lists none.
#[tokio::test]
async fn a_release_names_the_works_each_track_performs_in_order() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(ARTIST_ALBUM, "Album Artist A")
        .artist(ARTIST_COMPOSER, "Composer A")
        .album(ALBUM_A, "Album Title A", None, &[ARTIST_ALBUM])
        .release(ALBUM_A, RELEASE_A)
        .tracks(RELEASE_A, &[(TRACK_A, "Track A"), (TRACK_B, "Track B")])
        .work(WORK_A, "Zeta Work")
        .composer(WORK_A, ARTIST_COMPOSER)
        .work(WORK_NO_ARTISTS, "Alpha Work")
        .performs(TRACK_A, WORK_A)
        .performs(TRACK_A, WORK_NO_ARTISTS)
        .write(&test_records(&db))
        .await;

    let detail =
        crate::LibraryViews::new(db.clone()).find_release_detail(RELEASE_A).await.unwrap().unwrap();

    assert_eq!(
        detail.tracks[0].works,
        [
            crate::album_detail::TrackWork {
                work_id: WORK_A.to_string(),
                title: "Zeta Work".to_string(),
                composer_names: Some("Composer A".to_string()),
            },
            crate::album_detail::TrackWork {
                work_id: WORK_NO_ARTISTS.to_string(),
                title: "Alpha Work".to_string(),
                composer_names: None,
            },
        ]
    );
    assert!(detail.tracks[1].works.is_empty());
    assert!(detail.performs_works);
}

/// The detail of a release whose tracks perform no work has performs_works
/// false.
#[tokio::test]
async fn a_release_whose_tracks_perform_no_work_says_so() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(ARTIST_ALBUM, "Album Artist A")
        .album(ALBUM_A, "Album Title A", None, &[ARTIST_ALBUM])
        .release(ALBUM_A, RELEASE_A)
        .tracks(RELEASE_A, &[(TRACK_A, "Track A")])
        .write(&test_records(&db))
        .await;

    let detail =
        crate::LibraryViews::new(db.clone()).find_release_detail(RELEASE_A).await.unwrap().unwrap();

    assert!(!detail.performs_works);
}

/// A release's detail reports no catalog records until the test writes one,
/// and reports the record after.
#[tokio::test]
async fn a_release_says_whether_a_catalog_describes_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    RecordsFixture::new()
        .artist(ARTIST_ALBUM, "Album Artist A")
        .album(ALBUM_A, "Album Title A", None, &[ARTIST_ALBUM])
        .release(ALBUM_A, RELEASE_A)
        .tracks(RELEASE_A, &[(TRACK_A, "Track A")])
        .write(&records)
        .await;
    let views = crate::LibraryViews::new(db.clone());

    let before = views.find_release_detail(RELEASE_A).await.unwrap().unwrap();
    records
        .insert_release_records(
            RELEASE_A,
            &[bae_catalogs::records::ReleaseRecord::album(
                &bae_catalogs::records::MetadataRef::new(
                    bae_catalogs::catalog::Catalog::MusicBrainz,
                    "group-a",
                ),
            )],
        )
        .await
        .unwrap();
    let after = views.find_release_detail(RELEASE_A).await.unwrap().unwrap();

    assert!(!before.has_records);
    assert!(after.has_records);
    assert_eq!(after.records.len(), 1);
}

/// The library's creator has their profile from the moment the library exists
/// (§5.4), so the album's page offers them a rating before anyone rated: it
/// shows the rating row, with no stars of theirs, no one who rated the album,
/// and no stars on its tracks.
#[tokio::test]
async fn a_person_who_rated_nothing_can_rate_an_album_no_one_rated() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(ARTIST_ALBUM, "Album Artist A")
        .album(ALBUM_A, "Album Title A", None, &[ARTIST_ALBUM])
        .release(ALBUM_A, RELEASE_A)
        .tracks(RELEASE_A, &[(TRACK_A, "Track A")])
        .write(&test_records(&db))
        .await;

    let detail =
        crate::LibraryViews::new(db.clone()).find_album_detail(ALBUM_A).await.unwrap().unwrap();

    assert!(detail.rating.can_rate);
    assert!(detail.rating.shown);
    assert!(!detail.releases[0].has_other_files);
    assert_eq!(detail.rating.mine, None);
    assert!(detail.rating.raters.is_empty());
    assert_eq!(detail.releases[0].tracks[0].my_stars, None);
}

/// Searching with a limit keeps the first results in title order, tracks and
/// works included: the same ones the unlimited search lists first.
#[tokio::test]
async fn a_limited_search_keeps_the_first_titles() {
    let (db, _tmp) = crate::tests::library().await;
    let views = crate::LibraryViews::new(db.clone());
    let all = views.search_rows("1", 300).await.unwrap();
    let few = views.search_rows("1", 5).await.unwrap();
    let tracks = |found: &DbLibrarySearchResults| {
        found.tracks.iter().map(|track| track.id.clone()).take(5).collect::<Vec<_>>()
    };
    let works = |found: &DbLibrarySearchResults| {
        found.works.iter().map(|work| work.work.id.clone()).take(5).collect::<Vec<_>>()
    };
    assert_eq!(few.tracks.len(), 5);
    assert_eq!(tracks(&few), tracks(&all));
    assert_eq!(works(&few), works(&all));
}

/// A search reads its first tracks and works in title order from their
/// entries' title indexes, so it stops at its limit: the read that picks them
/// computes no title's key and sorts nothing, and the statement that reads
/// what each shows keeps that order without sorting again.
#[tokio::test]
async fn a_search_reads_its_tracks_and_works_in_order_from_the_title_indexes() {
    let (db, _tmp) = crate::tests::library().await;
    let plan = |statement: String| {
        let db = db.clone();
        async move {
            db.read(move |sql| {
                crate::tests::plan_steps(&sql, &statement, params![search_pattern("1"), 50])
            })
            .await
            .unwrap()
        }
    };
    for (kind, statement) in
        [("track", first_tracks_found_sql()), ("work", first_works_found_sql())]
    {
        let found = first_entries_found_sql(kind);
        assert!(!found.contains("bae_name_key"), "{found}");
        let steps = plan(found).await;
        let index = format!("USING COVERING INDEX idx_{kind}_search_entry_title");
        assert!(steps.iter().any(|step| step.detail.contains(&index)), "{steps:?}");
        assert!(!crate::tests::sorts_at_any_level(&steps), "{kind}: {steps:?}");

        let steps = plan(statement).await;
        assert!(steps.iter().any(|step| step.detail.contains(&index)), "{steps:?}");
        assert!(!crate::tests::scans_or_sorts(&steps), "{kind}: {steps:?}");
    }
}

/// A spoken "play" reads the tracks and the albums a search finds in order
/// from their stored title keys: each read walks its title index and sorts
/// nothing.
#[tokio::test]
async fn a_spoken_play_reads_tracks_and_albums_from_the_stored_title_keys() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    for (statement, index) in [
        (crate::album::top_track_statement(), "INDEX idx_track_search_entry_title"),
        (crate::album::top_album_track_statement(), "INDEX idx_album_listing_title"),
    ] {
        let plan = db
            .read(move |sql| crate::tests::plan_steps(&sql, &statement, ["Probe*"]))
            .await
            .unwrap();
        assert!(
            plan.iter().any(|step| step.detail.ends_with(index))
                && !plan.iter().any(|step| step.detail.contains("TEMP B-TREE")),
            "the read comes from {index} with no sort: {plan:?}"
        );
    }
}
