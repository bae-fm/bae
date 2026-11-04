//! The album views' live queries: the grid, its selection, an album's detail
//! and library search, each moved in place by its request and woken by the
//! writes that change what it shows, its own or another device's.

use super::*;
use bae_db::test_fixtures::aggregate_ordering::*;
use bae_db::test_fixtures::live_query::*;
use bae_db::test_fixtures::*;
use bae_db::test_fixtures::{exec, exec_batch};
use std::collections::BTreeSet;
use std::time::Duration;

/// The grid's selection reads through one subscription: its ids move in
/// place, it answers with the selected ids that name no album in the library,
/// and deleting a selected album wakes the read that shows it gone.
#[tokio::test]
async fn album_selection_follows_its_ids_on_one_subscription() {
    let (db, _temp) = live_db().await;
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
    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    db: &Database,
    live: &mut coven::ReconfigurableLiveQuery<bae_db::LibraryPageWindows, AlbumBrowseProjection>,
    shown: AlbumBrowseProjection,
    title: &str,
) -> AlbumBrowseProjection {
    let album_id = shown.windows[0].rows[0].album.id.clone();
    exec(db, "UPDATE albums SET title = ?1 WHERE id = ?2", &[title, album_id.as_str()]).await;
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    let (db, _temp) = live_db().await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], false, first_page());

    let initial = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.windows[0].rows[0].album.title, "Album Title");
    assert_eq!(initial.windows[0].rows[0].album.cover_version, None);

    let cover = bae_db::test_fixtures::write_cover(&db, RELEASE_ID, "cover bytes").await;

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    let (db, _temp) = live_db().await;
    exec(
        &db,
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES (?1, ?2, '2026-01-02T00:00:00Z', 0)",
        &[OTHER_RELEASE_ID, ALBUM_ID],
    )
    .await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], false, first_page());
    live.next().await.expect("the query stays open").result.unwrap();

    bae_db::test_fixtures::write_cover(&db, OTHER_RELEASE_ID, "other release cover bytes").await;
    let primary =
        bae_db::test_fixtures::write_cover(&db, RELEASE_ID, "primary release cover bytes").await;
    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the primary release's cover wakes the browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(updated.windows[0].rows[0].album.cover_version.as_deref(), Some(primary.as_str()));
}

#[tokio::test]
async fn album_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db().await;
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
    let renamed_event = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    let reordered = tokio::time::timeout(Duration::from_secs(2), live.next())
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
        bae_db::test_fixtures::write_cover(&db, OTHER_RELEASE_ID, "other cover bytes").await;
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

    let replaced_other_cover =
        bae_db::test_fixtures::write_cover(&db, OTHER_RELEASE_ID, "replaced other cover bytes")
            .await;
    let cover_updated = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    let (db, _temp) = live_db().await;
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
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
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
    let (db, _temp) = live_db().await;
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
    let (db, _temp) = live_db().await;
    bae_db::test_fixtures::write_cover(&db, RELEASE_ID, "cover bytes").await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(ALBUM_ID.to_string()));
    live.next().await.expect("the query stays open").result.unwrap();

    // The detail reads a cover's file, never its content type.
    exec(&db, "UPDATE covers SET content_type = 'image/png' WHERE id = ?1", &[RELEASE_ID]).await;

    next_run_is_only_the_request(&mut live, Some(OTHER_ALBUM_ID.to_string())).await;
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db().await;
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(ALBUM_ID.to_string()));
    assert!(live.next().await.expect("the query stays open").result.unwrap().detail.is_some());

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;

    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("album deletion wakes album detail")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(deleted.detail.is_none());
}

#[test]
fn album_page_subscription_delivers_a_write_materialized_by_sync() {
    bae_db::test_devices::run_two_store_test(async |devices| {
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
                db.find_album_by_id(ALBUM_ID).await.unwrap().is_some()
            })
            .await;
        let updated = tokio::time::timeout(Duration::from_secs(20), live.next())
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
    bae_db::test_devices::run_two_store_test(async |devices| {
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
                db.find_album_by_id(ALBUM_ID).await.unwrap().is_some()
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
                db.find_album_by_id(ALBUM_ID).await.unwrap().unwrap().title == "Renamed Album Title"
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
    let (db, _tmp) = aggregate_db().await;
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
