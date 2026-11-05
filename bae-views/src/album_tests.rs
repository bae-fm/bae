//! The album views' live queries: the grid, its selection, an album's detail
//! and library search, each moved in place by its request and woken by the
//! writes that change what it shows, its own or another device's.

use super::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::composed_work::*;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::live_query::*;
use bae_store::test_fixtures::*;
use bae_store::test_fixtures::{exec, exec_batch};
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

/// The grid's selection reads through one subscription: its ids move in
/// place, it answers with the selected ids that name no album in the library,
/// and deleting a selected album wakes the read that shows it gone.
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

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;
    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("deleting a selected album wakes the selection")
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
    live: &mut coven::ReconfigurableLiveQuery<bae_store::LibraryPageWindows, AlbumBrowseProjection>,
    shown: AlbumBrowseProjection,
    title: &str,
) -> AlbumBrowseProjection {
    let album_id = shown.windows[0].rows[0].album.id.clone();
    exec(db, "UPDATE albums SET title = ?1 WHERE id = ?2", &[title, album_id.as_str()]).await;
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
    exec(
        &db,
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES (?1, ?2, '2026-01-02T00:00:00Z', 0)",
        &[OTHER_RELEASE_ID, ALBUM_ID],
    )
    .await;
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
    let sort = [AlbumSortCriterion {
        field: AlbumSortField::DateAdded,
        direction: SortDirection::Ascending,
    }];
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&sort, false, BTreeSet::new());
    let requests = live.requests();
    let initial_event = live.next().await.expect("the query stays open");
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert!(initial.windows.is_empty());

    exec_batch(
        &db,
        &format!(
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{OTHER_ALBUM_ID}', 'Album Title Second', 0, '2026-01-02T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('4c8cbe06-0894-442b-b98c-adc509871a89', '{OTHER_ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-02T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{OTHER_RELEASE_ID}', '{OTHER_ALBUM_ID}', '2026-01-02T00:00:00Z', 0);"
        ),
    )
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

    exec(&db, "UPDATE albums SET title = 'Album Title Renamed' WHERE id = ?1", &[OTHER_ALBUM_ID])
        .await;
    let renamed_event = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("non-first album metadata wakes album browse")
        .expect("the query stays open");
    assert_eq!(renamed_event.revision, second_page_revision);
    let renamed = renamed_event.result.unwrap();
    assert_eq!(renamed.windows[0].rows[0].album.title, "Album Title Renamed");

    exec(
        &db,
        "UPDATE albums SET created_at = '2025-12-01T00:00:00Z' WHERE id = ?1",
        &[OTHER_ALBUM_ID],
    )
    .await;
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

    // The edited album is outside the requested window, so its metadata
    // changes nothing the browse delivers.
    exec(&db, "UPDATE albums SET title = 'Album Title Hidden' WHERE id = ?1", &[OTHER_ALBUM_ID])
        .await;
    let shown =
        next_album_page_is_only_a_rename(&db, &mut live, hidden_other_album, "Album Title Shown")
            .await;

    exec(
        &db,
        "UPDATE releases SET source_folder_name = 'unread folder' WHERE id = ?1",
        &[OTHER_RELEASE_ID],
    )
    .await;
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
    exec(&db, "DELETE FROM albums WHERE id = ?1", &[OTHER_ALBUM_ID]).await;
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

    exec(&db, "UPDATE albums SET title = 'Album Title Renamed' WHERE id = ?1", &[ALBUM_ID]).await;
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

    exec(
        &db,
        "INSERT INTO playback_state (id, current_entry, current_track_id, position_ms)
         VALUES ('current', NULL, NULL, 0)",
        &[],
    )
    .await;

    let window = LibraryPageWindow { offset: 0, limit: 49 };
    let moved = next_run_is_only_the_request(&mut live, [window].into_iter().collect())
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].rows, initial.windows[0].rows);
}

#[tokio::test]
async fn album_detail_subscription_ignores_an_unread_column() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    bae_records::test_fixtures::write_cover(&records, RELEASE_ID, "cover bytes").await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(ALBUM_ID.to_string()));
    live.next().await.expect("the query stays open").result.unwrap();

    // The detail reads a cover's file, never its content type.
    exec(&db, "UPDATE covers SET content_type = 'image/png' WHERE id = ?1", &[RELEASE_ID]).await;

    next_run_is_only_the_request(&mut live, Some(OTHER_ALBUM_ID.to_string())).await;
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(ALBUM_ID.to_string()));
    assert!(live.next().await.expect("the query stays open").result.unwrap().detail.is_some());

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;

    let deleted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("album deletion wakes album detail")
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
        exec_batch(writer, &format!(
            "INSERT INTO artists (id, name, name_key, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{ALBUM_ID}', 'Synced Album', 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('ec0d8e00-2141-4bae-8618-9d7ca8af03c4', '{ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{RELEASE_ID}', '{ALBUM_ID}', '2026-01-01T00:00:00Z', 0);"
        )).await;
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
        exec_batch(writer, &format!(
            "INSERT INTO artists (id, name, name_key, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{ALBUM_ID}', 'Écrin Album Title', 0, '2026-01-01T00:00:00Z');
             INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('b416679d-8454-4a3b-abc6-db8b32a35731', '{ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{RELEASE_ID}', '{ALBUM_ID}', '2026-01-01T00:00:00Z', 0);"
        )).await;
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

        exec(writer, "UPDATE albums SET title = 'Renamed Album Title' WHERE id = ?1", &[ALBUM_ID])
            .await;
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

    assert_eq!(summary.artist_names, "Artist Name Primary, Artist Name First, Artist Name Second");
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
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'artist name primary', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('7e9948c4-f2d0-4a73-8e5c-a885eda086ff', '50% Album Title', 2026, 0, '2026-01-01T00:00:00Z'),
             ('531c76e6-3ed8-45cd-8ee8-63366ef42031', '500 Album Title', 2026, 0, '2026-01-01T00:00:00Z'),
             ('2dd55a3f-3208-4faf-8737-453f474074cb', 'A_B Album Title', 2026, 0, '2026-01-01T00:00:00Z'),
             ('e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc', 'ACB Album Title', 2026, 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('40701200-73cb-4204-8bb7-f61757753770', '7e9948c4-f2d0-4a73-8e5c-a885eda086ff', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z'),
             ('be7cb5c6-a10e-4975-87d7-7672de8a06a2', '531c76e6-3ed8-45cd-8ee8-63366ef42031', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z'),
             ('87ee10fd-b497-4070-937a-a55da936c032', '2dd55a3f-3208-4faf-8737-453f474074cb', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z'),
             ('c51c5ff9-8982-4cfe-af7b-fcf775db7374', 'e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('94d062dd-9dc8-4e59-8c13-e5731b702157', '7e9948c4-f2d0-4a73-8e5c-a885eda086ff', '2026-01-01T00:00:00Z', 0),
             ('7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', '531c76e6-3ed8-45cd-8ee8-63366ef42031', '2026-01-01T00:00:00Z', 1),
             ('07d51116-f6e3-4b4c-8ae2-601d5a972bf9', '2dd55a3f-3208-4faf-8737-453f474074cb', '2026-01-01T00:00:00Z', 2),
             ('3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 'e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc', '2026-01-01T00:00:00Z', 3);

            INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES ('ba01e0d8-4915-5460-9357-8b0940ad2b39', '07d51116-f6e3-4b4c-8ae2-601d5a972bf9', 1, 'cd', 1, '2026-01-01T00:00:00Z'), ('c4a783fc-f0ff-5205-94e9-73ff657a62b5', '3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 1, 'cd', 1, '2026-01-01T00:00:00Z'), ('e8fd5ef8-51fd-5a5d-9311-5f318d6126c7', '7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', 1, 'cd', 1, '2026-01-01T00:00:00Z'), ('ec462973-8ca9-56a5-a434-8c055137e4b4', '94d062dd-9dc8-4e59-8c13-e5731b702157', 1, 'cd', 1, '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES ('4dc8cde9-15fb-470d-802c-b7e5f1ccc63d', '94d062dd-9dc8-4e59-8c13-e5731b702157', '50% Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', 'ec462973-8ca9-56a5-a434-8c055137e4b4'),
             ('37610729-bd6c-4511-81a9-8848a832ac73', '7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', '500 Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', 'e8fd5ef8-51fd-5a5d-9311-5f318d6126c7'),
             ('b2930937-dae6-4719-8150-aa61422eeeac', '07d51116-f6e3-4b4c-8ae2-601d5a972bf9', 'A_B Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', 'ba01e0d8-4915-5460-9357-8b0940ad2b39'),
             ('1f084b57-7c9c-457f-80fb-c61688c31175', '3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 'ACB Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', 'c4a783fc-f0ff-5205-94e9-73ff657a62b5');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

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
const SEARCHED_ALBUM: &str = "e307f0f5-e518-4d5e-bea8-83e439780ca0";
const SEARCHED_TRACK: &str = "b2140894-e938-4143-9c9a-5e054c2d7dde";
const SEARCHED_WORK: &str = "cef1d7ca-5e1b-4144-9e95-3ece6a68725e";

/// One album by one artist, on one release of one track performing one work
/// the artist wrote; every title and name starts with a letter outside ASCII.
const SEARCHED_LIBRARY: &str = "
    INSERT INTO artists (id, name, name_key, created_at) VALUES
        ('0373bb67-3abb-48e7-8a5b-6d29ef24308e', 'Åsel Artist Name', 'asel artist name', '2026-01-01T00:00:00Z');
    INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
        ('e307f0f5-e518-4d5e-bea8-83e439780ca0', 'Écrin Album Title', 2026, 0, '2026-01-01T00:00:00Z');
    INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES
        ('cd1bc541-a0b2-4440-8a6b-b9c1ffa33f67', 'e307f0f5-e518-4d5e-bea8-83e439780ca0', '0373bb67-3abb-48e7-8a5b-6d29ef24308e', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('45397b28-ee0e-4e87-921e-4fd7f9f07478', 'e307f0f5-e518-4d5e-bea8-83e439780ca0', '2026-01-01T00:00:00Z', 0);
    INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES
        ('941c9538-4220-4ff3-a5ed-a8bac3a6067d', '45397b28-ee0e-4e87-921e-4fd7f9f07478', 1, 'cd', 1, '2026-01-01T00:00:00Z');
    INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES
        ('b2140894-e938-4143-9c9a-5e054c2d7dde', '45397b28-ee0e-4e87-921e-4fd7f9f07478', 'Übel Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', '941c9538-4220-4ff3-a5ed-a8bac3a6067d');
    INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
        ('cef1d7ca-5e1b-4144-9e95-3ece6a68725e', 'Œvra Work Title', 'work', 'mb-work-oevra', '2026-01-01T00:00:00Z');
    INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
        ('47d0b5cc-ed3e-4deb-b3d4-20ed4268ec58', 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e', '0373bb67-3abb-48e7-8a5b-6d29ef24308e', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
    INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
        ('d2239bd6-e0b8-491c-8383-f444078a0cf1', 'b2140894-e938-4143-9c9a-5e054c2d7dde', 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
";

async fn exec_searched(db: &Store, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from)).await.unwrap();
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
    exec_searched(&db, SEARCHED_LIBRARY).await;

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
    exec_searched(&db, SEARCHED_LIBRARY).await;
    exec_searched(
        &db,
        "UPDATE albums SET title = 'Écrin*[x]?Album' WHERE id = 'e307f0f5-e518-4d5e-bea8-83e439780ca0';",
    )
    .await;

    assert_eq!(found(&db, "n*[x]?a").await[0], vec![SEARCHED_ALBUM.to_string()]);
    assert!(found(&db, "nx").await[0].is_empty());
    assert!(found(&db, "n?[").await[0].is_empty());
    assert!(found(&db, "*album").await[0].is_empty());
}

#[tokio::test]
async fn search_library_follows_names_as_they_change() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    exec_searched(&db, SEARCHED_LIBRARY).await;

    exec_searched(
        &db,
        "UPDATE artists SET name = 'Renamed Artist Name' WHERE id = '0373bb67-3abb-48e7-8a5b-6d29ef24308e';",
    )
    .await;
    assert!(found(&db, "åsel").await[0].is_empty());
    assert_eq!(found(&db, "renamed artist").await[0], vec![SEARCHED_ALBUM.to_string()]);

    // The credit prints another of the artist's names, which finds the album.
    exec_searched(
        &db,
        "INSERT INTO artist_credits (artist_id, name_key, name, created_at) VALUES
            ('0373bb67-3abb-48e7-8a5b-6d29ef24308e', 'printed name', 'Printed Näme', '2026-01-01T00:00:00Z');
         UPDATE album_artists SET credit_key = 'printed name' WHERE id = 'cd1bc541-a0b2-4440-8a6b-b9c1ffa33f67';",
    )
    .await;
    assert_eq!(found(&db, "printed näme").await[0], vec![SEARCHED_ALBUM.to_string()]);

    exec_searched(
        &db,
        "DELETE FROM albums WHERE id = 'e307f0f5-e518-4d5e-bea8-83e439780ca0';
         DELETE FROM works WHERE id = 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e';",
    )
    .await;
    let after = found(&db, "title").await;
    assert!(after[0].is_empty() && after[1].is_empty() && after[4].is_empty());
}
