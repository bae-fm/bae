//! The album views' live queries: the grid, its selection, an album's detail
//! and library search, each moved in place by its request and woken by the
//! writes that change what it shows, its own or another device's.

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
    assert!(live.next().await.expect("the query stays open").result.unwrap().is_empty());

    requests
        .set([ALBUM_ID.to_string(), "missing-album".to_string()].into_iter().collect())
        .unwrap();
    let selected = live.next().await.expect("the query stays open");
    assert_eq!(selected.request.len(), 2);
    assert_eq!(selected.result.unwrap(), vec!["missing-album"]);

    delete_release(&test_records(&db), RELEASE_ID).await;
    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("deleting a selected album's release wakes the selection")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted, vec![ALBUM_ID, "missing-album"]);
}

/// The window of the albums' first `limit` rows.
fn top(limit: u64) -> ListWindow {
    ListWindow::Forward { from: None, until: None, limit }
}

#[tokio::test]
async fn album_window_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let views = crate::LibraryViews::new(db.clone());
    let mut live = views.subscribe_album_window(&[], false, top(50));
    let counts = views.subscribe_album_count(false).next().await.unwrap().unwrap();
    assert_eq!(counts, AlbumCounts { albums: 1, rows: 1 });

    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.rows[0].album.title, "Album Title");
    assert_eq!(initial.rows[0].album.cover_version, None);

    let cover = bae_records::test_fixtures::write_cover(&records, RELEASE_ID, "cover bytes").await;

    let updated = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("cover write wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.rows[0].album.cover_version.as_deref(), Some(cover.as_str()));
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
    live.next().await.expect("the query stays open").result.unwrap();

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
    let updated = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("the primary release's cover wakes the browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.rows[0].album.cover_version.as_deref(), Some(primary.as_str()));
}

/// One window's request moves in place, each value naming the window it
/// answers; a write inside its range wakes it, one outside it does not, and a
/// row whose sort values move it out of the range leaves it.
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
    let initial_event = live.next().await.expect("the query stays open");
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.window, top(50));
    assert_eq!(
        initial.rows.iter().map(|row| row.album.id.as_str()).collect::<Vec<_>>(),
        [ALBUM_ID, OTHER_ALBUM_ID]
    );
    assert_eq!(initial.beyond, None);

    // The window from the second album on.
    let second = views.album_key_at(&sort, false, 1).await.unwrap().unwrap();
    let from_second = ListWindow::Forward { from: Some(second), until: None, limit: 50 };
    let revision = requests.set(from_second.clone()).unwrap();
    let event = live.next().await.expect("the query stays open");
    assert_eq!((&event.request, event.revision), (&from_second, revision));
    let moved = event.result.unwrap();
    assert_eq!(moved.window, from_second);
    assert_eq!(moved.rows.len(), 1);
    assert_eq!(moved.rows[0].album.id, OTHER_ALBUM_ID);

    // An edit that keeps the album in the range wakes it.
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Title Second Renamed").await.unwrap();
    let renamed = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("an album in the window's range wakes it")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(renamed.rows[0].album.title, "Album Title Second Renamed");

    // A cover on an album in the range wakes it.
    let cover_in_range =
        bae_records::test_fixtures::write_cover(&records, OTHER_RELEASE_ID, "inside").await;
    let covered = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a cover of an album in the range wakes the window")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(covered.rows[0].album.cover_version.as_deref(), Some(cover_in_range.as_str()));

    // A title that sorts the album above the window's start takes it out of
    // the range: the window delivers none, and the album's edits after that
    // are outside it.
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Title").await.unwrap();
    let left = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("an album leaving the range wakes the window")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(left.rows.is_empty());
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Title Again").await.unwrap();
    assert!(!live.is_marked_for_rerun(), "an album that left the range");

    // The top of the list holds it again, first.
    requests.set(top(50)).unwrap();
    let top_again = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(top_again.rows[0].album.id, OTHER_ALBUM_ID);

    delete_release(&records, OTHER_RELEASE_ID).await;
    let deleted = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(deleted.rows.len(), 1);
    assert_eq!(views.subscribe_album_count(false).next().await.unwrap().unwrap().rows, 1);
}

#[tokio::test]
async fn album_window_ignores_an_unread_table() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_window(&[], false, top(50));
    let initial = live.next().await.expect("the query stays open").result.unwrap();

    bae_playback::tables::PlaybackQueueTables::new(db.clone())
        .save_playback_state(&bae_playback::tables::PlaybackResumeRow {
            current: None,
            position_ms: Some(0),
        })
        .await
        .unwrap();

    assert!(!live.is_marked_for_rerun(), "a table the window does not read");
    let moved = next_run_is_only_the_request(&mut live, top(49)).await.result.unwrap();
    assert_eq!(moved.rows, initial.rows);
}

/// An album's page reads its album, its artists, every release with all its
/// parts, the primary release and the person's ratings: five statements,
/// whatever number of releases the album has.
#[tokio::test]
async fn album_detail_reads_every_release_in_one_statement() {
    let (db, _temp) =
        bae_records::test_fixtures::aggregate_ordering::aggregate_db(crate::TEST_SCHEMA).await;
    let log = db.record_statements_for_test().await.unwrap();
    let detail = crate::LibraryViews::new(db.clone())
        .find_album_detail(bae_records::test_fixtures::ALBUM_A)
        .await
        .unwrap()
        .expect("the album has releases");
    assert_eq!(detail.releases.len(), 3, "the album has three releases");
    assert!(detail.shows_release_choices, "an album of three releases offers a choice");
    let statements = log.take();
    assert_eq!(statements.len(), 5, "{} statements", statements.len());
}

/// An album of one release offers no choice between releases.
#[tokio::test]
async fn an_album_of_one_release_shows_no_release_choices() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let detail = crate::LibraryViews::new(db.clone())
        .find_album_detail(ALBUM_ID)
        .await
        .unwrap()
        .expect("the album has a release");
    assert_eq!(detail.releases.len(), 1);
    assert!(!detail.shows_release_choices);
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(ALBUM_ID.to_string()));
    assert!(live.next().await.expect("the query stays open").result.unwrap().detail.is_some());

    delete_release(&test_records(&db), RELEASE_ID).await;

    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("deleting the album's one release wakes album detail")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(deleted.detail.is_none());
}

#[test]
fn album_page_subscription_delivers_a_write_materialized_by_sync() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let writer = devices.a();
        let reader = devices.b();
        let mut live =
            crate::LibraryViews::new(reader.clone()).subscribe_album_window(&[], false, top(50));
        assert!(live.next().await.expect("the query stays open").result.unwrap().rows.is_empty());
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
        let updated = tokio::time::timeout(HANG_GUARD, live.next())
            .await
            .unwrap()
            .expect("the query stays open")
            .result
            .unwrap();
        assert_eq!(updated.rows[0].album.title, "Synced Album");
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

    let work_results =
        crate::LibraryViews::new(db.clone()).search_rows("Displayed Work", 10).await.unwrap();
    assert_eq!(work_results.works.len(), 1);
    assert_eq!(work_results.works[0].work.id, WORK_CHILD_A);
    assert_eq!(work_results.works[0].representative_release_id.as_deref(), Some(RELEASE_A));
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
}

/// A person with no profile cannot rate: the album's page reads that, no one
/// who rated it, and no stars on its tracks.
#[tokio::test]
async fn a_person_without_a_profile_reads_no_ratings() {
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

    assert!(!detail.rating.can_rate);
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
