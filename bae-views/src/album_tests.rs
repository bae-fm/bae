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

/// Rename the one album `shown` holds, and assert the next page delivered is
/// `shown` with that title and nothing else. The browse reads every commit
/// since its last page in one run, so a change it should not follow, committed
/// before the rename, would show in that page too.
async fn next_album_page_is_only_a_rename(
    db: &Store,
    live: &mut coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        AlbumBrowseProjection,
        bae_store::DbError,
    >,
    shown: AlbumBrowseProjection,
    title: &str,
) -> AlbumBrowseProjection {
    let album_id = shown.windows[0].rows[0].album.id.clone();
    test_records(db).rename_album_for_test(&album_id, title).await.unwrap();
    let renamed = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("renaming the shown album wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    let mut expected = shown;
    expected.windows[0].rows[0].album.title = title.to_string();
    assert_eq!(renamed, expected, "the page carries more than the rename");
    renamed
}

#[tokio::test]
async fn album_browse_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], false, first_page());

    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.windows[0].rows[0].album.title, "Album Title");
    assert_eq!(initial.windows[0].rows[0].album.cover_version, None);

    let cover = bae_records::test_fixtures::write_cover(&records, RELEASE_ID, "cover bytes").await;

    let updated = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("cover write wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.windows[0].rows[0].album.cover_version.as_deref(), Some(cover.as_str()));
}

/// A grid card shows its album's primary release's cover, so that is the one
/// cover the browse reads: another release of the album gaining a cover is
/// not a change to the grid.
#[tokio::test]
async fn album_browse_reads_only_the_primary_releases_cover() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    records.insert_release(&DbRelease::new_test(ALBUM_ID, OTHER_RELEASE_ID)).await.unwrap();
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], false, first_page());
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
    assert_eq!(updated.windows[0].rows[0].album.cover_version.as_deref(), Some(primary.as_str()));
}

#[tokio::test]
async fn album_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&sort, false, BTreeSet::new());
    let requests = live.requests();
    let initial_event = live.next().await.expect("the query stays open");
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert!(initial.windows.is_empty());

    RecordsFixture::new()
        .album(OTHER_ALBUM_ID, "Album Title Second", None, &[ARTIST_ID])
        .release(OTHER_ALBUM_ID, OTHER_RELEASE_ID)
        .write(&records)
        .await;
    let inserted = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(inserted.total_count, 2);
    assert!(inserted.windows.is_empty());

    let first_window = LibraryPageWindow { offset: 0, limit: 1 };
    let second_window = LibraryPageWindow { offset: 1, limit: 1 };
    let requested =
        [first_window.clone(), second_window.clone()].into_iter().collect::<BTreeSet<_>>();
    let pages_revision = requests.set(requested.clone()).unwrap();
    let pages_event = live.next().await.expect("the query stays open");
    assert_eq!(pages_event.request, requested);
    assert_eq!(pages_event.revision, pages_revision);
    let pages = pages_event.result.unwrap();
    assert_eq!(
        pages
            .windows
            .iter()
            .map(|window| (window.window.clone(), window.rows[0].album.id.as_str()))
            .collect::<Vec<_>>(),
        vec![(first_window.clone(), ALBUM_ID), (second_window.clone(), OTHER_ALBUM_ID),]
    );

    let second_page = [second_window.clone()].into_iter().collect::<BTreeSet<_>>();
    let second_page_revision = requests.set(second_page.clone()).unwrap();
    let second_page_event = live.next().await.expect("the query stays open");
    assert_eq!(second_page_event.request, second_page);
    assert_eq!(second_page_event.revision, second_page_revision);
    let second_page_initial = second_page_event.result.unwrap();
    assert_eq!(second_page_initial.windows[0].rows[0].album.id, OTHER_ALBUM_ID);

    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Title Renamed").await.unwrap();
    let renamed_event = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("non-first album metadata wakes album browse")
        .expect("the query stays open");
    assert_eq!(renamed_event.revision, second_page_revision);
    let renamed = renamed_event.result.unwrap();
    assert_eq!(renamed.windows[0].rows[0].album.title, "Album Title Renamed");

    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Title").await.unwrap();
    let reordered = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("non-first album ordering field wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(reordered.windows[0].rows[0].album.id, ALBUM_ID);

    let first_page = [first_window].into_iter().collect::<BTreeSet<_>>();
    requests.set(first_page).unwrap();
    let reordered_first = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(reordered_first.windows[0].rows[0].album.id, OTHER_ALBUM_ID);

    requests.set([second_window].into_iter().collect::<BTreeSet<_>>()).unwrap();
    let hidden_other_album = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(hidden_other_album.windows[0].rows[0].album.id, ALBUM_ID);

    // The edited album is outside the requested window, and its new title
    // keeps it there, so its metadata changes nothing the browse delivers.
    records.rename_album_for_test(OTHER_ALBUM_ID, "Album Earlier Hidden").await.unwrap();
    let shown =
        next_album_page_is_only_a_rename(&db, &mut live, hidden_other_album, "Album Title Shown")
            .await;

    records
        .save_release_metadata_for_test(OTHER_RELEASE_ID, |rows| {
            rows.release.release_name = Some("Unread Release Name".to_string());
        })
        .await
        .unwrap();
    let second_and_beyond =
        [LibraryPageWindow { offset: 1, limit: 2 }].into_iter().collect::<BTreeSet<_>>();
    let shown_again =
        next_run_is_only_the_request(&mut live, second_and_beyond).await.result.unwrap();
    assert_eq!(shown_again.windows[0].rows, shown.windows[0].rows);
    let shown = shown_again;

    let other_cover =
        bae_records::test_fixtures::write_cover(&records, OTHER_RELEASE_ID, "other cover bytes")
            .await;
    // Only the requested rows' covers are read, so a cover on the album
    // outside the window changes nothing the browse delivers.
    next_album_page_is_only_a_rename(&db, &mut live, shown, "Album Title Shown Again").await;
    let other_album_page =
        [LibraryPageWindow { offset: 0, limit: 1 }].into_iter().collect::<BTreeSet<_>>();
    requests.set(other_album_page).unwrap();
    let covered = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(covered.windows[0].rows[0].album.id, OTHER_ALBUM_ID);
    assert_eq!(
        covered.windows[0].rows[0].album.cover_version.as_deref(),
        Some(other_cover.as_str())
    );

    let replaced_other_cover = bae_records::test_fixtures::write_cover(
        &records,
        OTHER_RELEASE_ID,
        "replaced other cover bytes",
    )
    .await;
    let cover_updated = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a requested album's cover version wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        cover_updated.windows[0].rows[0].album.cover_version.as_deref(),
        Some(replaced_other_cover.as_str())
    );

    requests
        .set([LibraryPageWindow { offset: 1, limit: 1 }].into_iter().collect::<BTreeSet<_>>())
        .unwrap();
    live.next().await.expect("the query stays open").result.unwrap();
    delete_release(&records, OTHER_RELEASE_ID).await;
    let deleted = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(deleted.total_count, 1);
    assert!(deleted.windows[0].rows.is_empty());
}

/// A search follows its text by moving its request: the same subscription
/// answers each query, a blank one reads nothing, and a commit to a row the
/// current query found wakes it.
#[tokio::test]
async fn library_search_moves_between_queries_on_one_subscription() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_library_search(None, 50);
    let requests = live.requests();

    let idle = live.next().await.expect("the query stays open").result.unwrap();
    assert!(idle.results.albums.is_empty(), "no query finds nothing");

    requests.set(crate::LibrarySearchQuery::parse("Album Title")).unwrap();
    let found = live.next().await.expect("the query stays open");
    assert_eq!(found.request.as_ref().map(|query| query.as_str()), Some("Album Title"));
    let found = found.result.unwrap();
    assert_eq!(found.results.albums.len(), 1);
    assert_eq!(found.results.albums[0].id, ALBUM_ID);

    test_records(&db).rename_album_for_test(ALBUM_ID, "Album Title Renamed").await.unwrap();
    let renamed = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a found album's edit wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(renamed.results.albums[0].title, "Album Title Renamed");

    requests.set(crate::LibrarySearchQuery::parse("   ")).unwrap();
    let cleared = live.next().await.expect("the query stays open").result.unwrap();
    assert!(cleared.results.albums.is_empty(), "a blank query is no search");
}

#[tokio::test]
async fn album_browse_ignores_an_unread_table() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], false, first_page());
    let initial = live.next().await.expect("the query stays open").result.unwrap();

    bae_playback::tables::PlaybackQueueTables::new(db.clone())
        .save_playback_state(&bae_playback::tables::PlaybackResumeRow {
            current: None,
            position_ms: Some(0),
        })
        .await
        .unwrap();

    let window = LibraryPageWindow { offset: 0, limit: 49 };
    let moved = next_run_is_only_the_request(&mut live, [window].into_iter().collect())
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].rows, initial.windows[0].rows);
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
        let mut live = crate::LibraryViews::new(reader.clone()).subscribe_album_browse(
            &[],
            false,
            first_page(),
        );
        assert_eq!(live.next().await.expect("the query stays open").result.unwrap().total_count, 0);
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
        assert_eq!(updated.windows[0].rows[0].album.title, "Synced Album");
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

    // Deleting the album's one release takes its track; the album, empty, and
    // the work stay, and search finds no album that holds no release.
    delete_release(&test_records(&db), SEARCHED_RELEASE).await;
    let after = found(&db, "title").await;
    assert!(after[0].is_empty() && after[1].is_empty());
    assert_eq!(after[4], vec![SEARCHED_WORK.to_string()]);
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
