use super::super::*;
use super::{exec, exec_batch};
use crate::library::LibraryPageWindow;
use std::collections::BTreeSet;
use std::time::Duration;

const ARTIST_ID: &str = "96f0ef68-c284-4f74-b3f6-d9a4b48ee6d1";
pub(super) const ALBUM_ID: &str = "975b724f-fce7-4fdb-85be-9f57fd9ba496";
pub(super) const RELEASE_ID: &str = "44751464-1552-44be-8c5a-955cc5b61d12";
const OTHER_ALBUM_ID: &str = "5cf43aa4-8374-4ec4-bf08-239685a62f37";
pub(super) const IDENTITY_ID: &str = "d9af7374-f7de-4d5b-8f9e-19b250ca2693";
const OTHER_RELEASE_ID: &str = "79764470-a937-42fd-bbd4-fce67651c72e";
const COMPOSER_ID: &str = "735c571d-5dcf-4a52-af70-f080f6a82a2d";
const OTHER_COMPOSER_ID: &str = "041df287-6a78-4db4-981d-70eb24bad7ec";
const INSERTED_COMPOSER_ID: &str = "7cc478ba-dd73-4b9f-9739-a1524088800e";
const COMPOSER_WORK_ID: &str = "eb9cf137-3917-408a-9e4f-670c5f39051d";
const OTHER_COMPOSER_WORK_ID: &str = "9958dcbd-ebac-42b9-9b6e-77bc421ac32b";
const INSERTED_COMPOSER_WORK_ID: &str = "e1aa84de-4495-499c-92f8-9ff6ccdc59f5";
const COMPOSER_LINK_ID: &str = "8621fd15-044a-44dd-ae70-dd6de30e35da";
const OTHER_COMPOSER_LINK_ID: &str = "49a10300-3036-4301-9e1b-a50a76a46032";
const INSERTED_COMPOSER_LINK_ID: &str = "9168d755-3ed6-446c-a0ec-5f4039e18a6f";

pub(super) async fn live_db() -> (Database, tempfile::TempDir) {
    let (db, temp) = super::temp_db().await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES ('{ARTIST_ID}', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{ALBUM_ID}', 'Album Title', 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('22586998-2ee2-4c3d-a50d-696cc9c39b4e', '{ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{RELEASE_ID}', '{ALBUM_ID}', '2026-01-01T00:00:00Z', 0);"
        ),
    )
    .await;
    (db, temp)
}

/// Store an artist image holding `content` and return the version reads
/// report for it.
async fn write_artist_image(db: &Database, artist: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    db.write_library_image(
        &DbLibraryImage {
            id: artist.into(),
            image_type: LibraryImageType::Artist,
            content_type: ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: "file_tags".into(),
            source_url: None,
            created_at: super::fixed_now(),
            revision: uuid::Uuid::new_v4().to_string(),
        },
        bytes,
    )
    .await
    .unwrap();
    super::stored_image_version(db, crate::sync::ARTIST_IMAGES_NAMESPACE, artist).await
}

/// Move `live` to `request` and return the event answering it, asserting the
/// move alone caused that run. Every commit since the query's last value is
/// read into the same run, so one touching anything the query reads makes the
/// cause `RequestAndDatabaseChanged`.
pub(super) async fn next_run_is_only_the_request<Request, Value>(
    live: &mut coven::ReconfigurableLiveQuery<Request, Value>,
    request: Request,
) -> coven::ReconfigurableLiveQueryEvent<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    let revision = live
        .requests()
        .set(request)
        .expect("the subscription is open");
    let event = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a new request reruns the query")
        .expect("the query stays open");
    assert_eq!(event.revision, revision);
    assert_eq!(
        event.cause,
        coven::LiveQueryCause::Request,
        "a commit since the last value touched what the query reads"
    );
    event
}

/// Rename the one album `shown` holds, and assert the next page delivered is
/// `shown` with that title and nothing else. The browse reads every commit
/// since its last page in one run, so a change it should not follow, committed
/// before the rename, would show in that page too.
async fn next_album_page_is_only_a_rename(
    db: &Database,
    live: &mut coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        AlbumBrowseProjection,
    >,
    shown: AlbumBrowseProjection,
    title: &str,
) -> AlbumBrowseProjection {
    let album_id = shown.windows[0].rows[0].id.clone();
    exec(
        db,
        "UPDATE albums SET title = ?1 WHERE id = ?2",
        &[title, album_id.as_str()],
    )
    .await;
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("renaming the shown album wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    let mut expected = shown;
    expected.windows[0].rows[0].title = title.to_string();
    assert_eq!(renamed, expected, "the page carries more than the rename");
    renamed
}

/// [`next_album_page_is_only_a_rename`] for the composer grid: rename the one
/// composer `shown` holds.
async fn next_composer_page_is_only_a_rename(
    db: &Database,
    live: &mut coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        ComposerBrowseProjection,
    >,
    shown: ComposerBrowseProjection,
    name: &str,
) -> ComposerBrowseProjection {
    let composer_id = shown.windows[0].rows[0].artist.id.clone();
    exec(
        db,
        "UPDATE artists SET name = ?1 WHERE id = ?2",
        &[name, composer_id.as_str()],
    )
    .await;
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("renaming the shown composer wakes composer browse")
        .expect("the query stays open")
        .result
        .unwrap();
    let mut expected = shown;
    expected.windows[0].rows[0].artist.name = name.to_string();
    assert_eq!(renamed, expected, "the page carries more than the rename");
    renamed
}

/// The first page of a list, as the one window a browse query reads.
fn first_page() -> BTreeSet<LibraryPageWindow> {
    [LibraryPageWindow {
        offset: 0,
        limit: 50,
    }]
    .into_iter()
    .collect()
}

#[tokio::test]
async fn album_browse_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_browse(&[], false, first_page());

    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.windows[0].rows[0].title, "Album Title");
    assert!(initial.cover_versions.is_empty());

    let cover = super::write_cover(&db, RELEASE_ID, "cover bytes").await;

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("cover write wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        updated.cover_versions.get(RELEASE_ID).map(String::as_str),
        Some(cover.as_str())
    );
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
    let mut live = db.subscribe_album_browse(&[], false, first_page());
    live.next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();

    super::write_cover(&db, OTHER_RELEASE_ID, "other release cover bytes").await;
    super::write_cover(&db, RELEASE_ID, "primary release cover bytes").await;
    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the primary release's cover wakes the browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        updated.cover_versions.keys().collect::<Vec<_>>(),
        vec![RELEASE_ID]
    );
}

#[tokio::test]
async fn album_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db().await;
    let sort = [AlbumSortCriterion {
        field: AlbumSortField::DateAdded,
        direction: SortDirection::Ascending,
    }];
    let mut live = db.subscribe_album_browse(&sort, false, BTreeSet::new());
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
    let inserted = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(inserted.total_count, 2);
    assert!(inserted.windows.is_empty());

    let first_window = LibraryPageWindow {
        offset: 0,
        limit: 1,
    };
    let second_window = LibraryPageWindow {
        offset: 1,
        limit: 1,
    };
    let requested = [first_window.clone(), second_window.clone()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let pages_revision = requests.set(requested.clone()).unwrap();
    let pages_event = live.next().await.expect("the query stays open");
    assert_eq!(pages_event.request, requested);
    assert_eq!(pages_event.revision, pages_revision);
    let pages = pages_event.result.unwrap();
    assert_eq!(
        pages
            .windows
            .iter()
            .map(|window| (window.window.clone(), window.rows[0].id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (first_window.clone(), ALBUM_ID),
            (second_window.clone(), OTHER_ALBUM_ID),
        ]
    );

    let second_page = [second_window.clone()].into_iter().collect::<BTreeSet<_>>();
    let second_page_revision = requests.set(second_page.clone()).unwrap();
    let second_page_event = live.next().await.expect("the query stays open");
    assert_eq!(second_page_event.request, second_page);
    assert_eq!(second_page_event.revision, second_page_revision);
    let second_page_initial = second_page_event.result.unwrap();
    assert_eq!(second_page_initial.windows[0].rows[0].id, OTHER_ALBUM_ID);

    exec(
        &db,
        "UPDATE albums SET title = 'Album Title Renamed' WHERE id = ?1",
        &[OTHER_ALBUM_ID],
    )
    .await;
    let renamed_event = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first album metadata wakes album browse")
        .expect("the query stays open");
    assert_eq!(renamed_event.revision, second_page_revision);
    let renamed = renamed_event.result.unwrap();
    assert_eq!(renamed.windows[0].rows[0].title, "Album Title Renamed");

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
    assert_eq!(reordered.windows[0].rows[0].id, ALBUM_ID);

    let first_page = [first_window].into_iter().collect::<BTreeSet<_>>();
    requests.set(first_page).unwrap();
    let reordered_first = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(reordered_first.windows[0].rows[0].id, OTHER_ALBUM_ID);

    requests
        .set([second_window].into_iter().collect::<BTreeSet<_>>())
        .unwrap();
    let hidden_other_album = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(hidden_other_album.windows[0].rows[0].id, ALBUM_ID);

    // The edited album is outside the requested window, so its metadata
    // changes nothing the browse delivers.
    exec(
        &db,
        "UPDATE albums SET title = 'Album Title Hidden' WHERE id = ?1",
        &[OTHER_ALBUM_ID],
    )
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
    let second_and_beyond = [LibraryPageWindow {
        offset: 1,
        limit: 2,
    }]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let shown_again = next_run_is_only_the_request(&mut live, second_and_beyond)
        .await
        .result
        .unwrap();
    assert_eq!(shown_again.windows[0].rows, shown.windows[0].rows);
    let shown = shown_again;

    let other_cover = super::write_cover(&db, OTHER_RELEASE_ID, "other cover bytes").await;
    // Only the requested rows' covers are read, so a cover on the album
    // outside the window changes nothing the browse delivers.
    next_album_page_is_only_a_rename(&db, &mut live, shown, "Album Title Shown Again").await;
    let other_album_page = [LibraryPageWindow {
        offset: 0,
        limit: 1,
    }]
    .into_iter()
    .collect::<BTreeSet<_>>();
    requests.set(other_album_page).unwrap();
    let covered = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(covered.windows[0].rows[0].id, OTHER_ALBUM_ID);
    assert_eq!(
        covered
            .cover_versions
            .get(OTHER_RELEASE_ID)
            .map(String::as_str),
        Some(other_cover.as_str())
    );

    let replaced_other_cover =
        super::write_cover(&db, OTHER_RELEASE_ID, "replaced other cover bytes").await;
    let cover_updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a requested album's cover version wakes album browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        cover_updated
            .cover_versions
            .get(OTHER_RELEASE_ID)
            .map(String::as_str),
        Some(replaced_other_cover.as_str())
    );

    requests
        .set(
            [LibraryPageWindow {
                offset: 1,
                limit: 1,
            }]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        )
        .unwrap();
    live.next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    exec(&db, "DELETE FROM albums WHERE id = ?1", &[OTHER_ALBUM_ID]).await;
    let deleted = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 1);
    assert!(deleted.windows[0].rows.is_empty());
}

#[tokio::test]
async fn composer_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db().await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES ('{COMPOSER_ID}', 'Composer Name First', 'composer name first', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_ID}', 'Composer Name Second', 'composer name second', '2026-01-02T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('{COMPOSER_WORK_ID}', 'Work Title First', 'work', 'work-first', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_WORK_ID}', 'Work Title Second', 'work', 'work-second', '2026-01-02T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('{COMPOSER_LINK_ID}', '{COMPOSER_WORK_ID}', '{COMPOSER_ID}', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_LINK_ID}', '{OTHER_COMPOSER_WORK_ID}', '{OTHER_COMPOSER_ID}', 0, 'file_tags', '2026-01-02T00:00:00Z');"
        ),
    )
    .await;
    let sort = [ComposerSortCriterion {
        field: ComposerSortField::Name,
        direction: SortDirection::Descending,
    }];
    let second_window = LibraryPageWindow {
        offset: 1,
        limit: 1,
    };
    let initial_request = [second_window.clone()].into_iter().collect::<BTreeSet<_>>();
    let mut live = db.subscribe_composer_browse(&sort, initial_request.clone());
    let requests = live.requests();
    let initial_event = live.next().await.expect("the query stays open");
    assert_eq!(initial_event.request, initial_request);
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.total_count, 2);
    assert_eq!(initial.windows[0].rows[0].artist.id, COMPOSER_ID);

    let both_pages = [
        LibraryPageWindow {
            offset: 0,
            limit: 1,
        },
        second_window.clone(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let both_pages_revision = requests.set(both_pages.clone()).unwrap();
    let both_event = live.next().await.expect("the query stays open");
    assert_eq!(both_event.revision, both_pages_revision);
    let both = both_event.result.unwrap();
    assert_eq!(both.windows.len(), 2);
    assert_eq!(both.windows[0].rows[0].artist.id, OTHER_COMPOSER_ID);
    assert_eq!(both.windows[1].rows[0].artist.id, COMPOSER_ID);

    exec(
        &db,
        "UPDATE artists SET name = 'Composer Name Renamed', name_key = 'composer name renamed' WHERE id = ?1",
        &[OTHER_COMPOSER_ID],
    )
    .await;
    let renamed_event = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first composer metadata wakes composer browse")
        .expect("the query stays open");
    assert_eq!(renamed_event.revision, both_pages_revision);
    let renamed = renamed_event.result.unwrap();
    assert_eq!(
        renamed
            .windows
            .iter()
            .map(|window| window.rows[0].artist.id.as_str())
            .collect::<Vec<_>>(),
        vec![OTHER_COMPOSER_ID, COMPOSER_ID]
    );
    assert_eq!(
        renamed.windows[0].rows[0].artist.name,
        "Composer Name Renamed"
    );

    requests
        .set([second_window].into_iter().collect::<BTreeSet<_>>())
        .unwrap();
    let hidden_other_composer = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        hidden_other_composer.windows[0].rows[0].artist.id,
        COMPOSER_ID
    );

    // The edited composer is outside the requested window, so its metadata
    // changes nothing the browse delivers.
    exec(
        &db,
        "UPDATE artists SET sort_name = 'Composer Sort Hidden' WHERE id = ?1",
        &[OTHER_COMPOSER_ID],
    )
    .await;
    let shown = next_composer_page_is_only_a_rename(
        &db,
        &mut live,
        hidden_other_composer,
        "Composer Name First Shown",
    )
    .await;

    let composer_image = write_artist_image(&db, OTHER_COMPOSER_ID, "composer image bytes").await;
    // Only the requested rows' images are read, so an image on the composer
    // outside the window changes nothing the browse delivers.
    next_composer_page_is_only_a_rename(&db, &mut live, shown, "Composer Name First Again").await;
    let other_composer_page = [LibraryPageWindow {
        offset: 0,
        limit: 1,
    }]
    .into_iter()
    .collect::<BTreeSet<_>>();
    requests.set(other_composer_page).unwrap();
    let imaged = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(imaged.windows[0].rows[0].artist.id, OTHER_COMPOSER_ID);
    assert_eq!(
        imaged
            .image_versions
            .get(OTHER_COMPOSER_ID)
            .map(String::as_str),
        Some(composer_image.as_str())
    );

    let replaced_composer_image =
        write_artist_image(&db, OTHER_COMPOSER_ID, "replaced composer image bytes").await;
    let image_updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a requested composer's image version wakes composer browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        image_updated
            .image_versions
            .get(OTHER_COMPOSER_ID)
            .map(String::as_str),
        Some(replaced_composer_image.as_str())
    );

    requests.set(BTreeSet::new()).unwrap();
    let count_only = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(count_only.total_count, 2);
    assert!(count_only.windows.is_empty());

    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES ('{INSERTED_COMPOSER_ID}', 'Composer Name Inserted', 'composer name inserted', '2026-01-03T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('{INSERTED_COMPOSER_WORK_ID}', 'Work Title Inserted', 'work', 'work-inserted', '2026-01-03T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('{INSERTED_COMPOSER_LINK_ID}', '{INSERTED_COMPOSER_WORK_ID}', '{INSERTED_COMPOSER_ID}', 0, 'file_tags', '2026-01-03T00:00:00Z');"
        ),
    )
    .await;
    let inserted = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(inserted.total_count, 3);
    assert!(inserted.windows.is_empty());

    exec(
        &db,
        "DELETE FROM work_artists WHERE id = ?1",
        &[INSERTED_COMPOSER_LINK_ID],
    )
    .await;
    let deleted = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(deleted.total_count, 2);
    assert!(deleted.windows.is_empty());
}

/// The artist grid reads the artists its windows hold, and follows the images
/// of those artists only: an image on an artist outside every window changes
/// nothing it delivers.
#[tokio::test]
async fn artist_browse_reads_its_windows_and_their_images() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_artist_browse(&[], first_page());

    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.windows[0].rows[0].artist.id, ARTIST_ID);
    assert!(initial.image_versions.is_empty());

    let artist_image = write_artist_image(&db, ARTIST_ID, "artist image bytes").await;
    let imaged = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("an image on a listed artist wakes artist browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        imaged.image_versions.get(ARTIST_ID).map(String::as_str),
        Some(artist_image.as_str())
    );

    live.requests().set(BTreeSet::new()).unwrap();
    let count_only = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(count_only.total_count, 1);
    assert!(count_only.windows.is_empty());
    assert!(count_only.image_versions.is_empty());
}

/// A search follows its text by moving its request: the same subscription
/// answers each query, a blank one reads nothing, and a commit to a row the
/// current query found wakes it.
#[tokio::test]
async fn library_search_moves_between_queries_on_one_subscription() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_library_search(None, 50);
    let requests = live.requests();

    let idle = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(idle.results.albums.is_empty(), "no query finds nothing");

    requests
        .set(crate::library::LibrarySearchQuery::parse("Album Title"))
        .unwrap();
    let found = live.next().await.expect("the query stays open");
    assert_eq!(
        found.request.as_ref().map(|query| query.as_str()),
        Some("Album Title")
    );
    let found = found.result.unwrap();
    assert_eq!(found.results.albums.len(), 1);
    assert_eq!(found.results.albums[0].id, ALBUM_ID);

    exec(
        &db,
        "UPDATE albums SET title = 'Album Title Renamed' WHERE id = ?1",
        &[ALBUM_ID],
    )
    .await;
    let renamed = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("a found album's edit wakes the search")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(renamed.results.albums[0].title, "Album Title Renamed");

    requests
        .set(crate::library::LibrarySearchQuery::parse("   "))
        .unwrap();
    let cleared = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(
        cleared.results.albums.is_empty(),
        "a blank query is no search"
    );
}

#[tokio::test]
async fn album_browse_ignores_an_unread_table() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_browse(&[], false, first_page());
    let initial = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();

    exec(
        &db,
        "INSERT INTO playback_state
         (id, source, shuffled, manual, repeat, current_track_id, position_ms, volume, is_muted)
         VALUES ('current', NULL, NULL, '[]', 'off', NULL, 0, 1.0, 0)",
        &[],
    )
    .await;

    let window = LibraryPageWindow {
        offset: 0,
        limit: 49,
    };
    let moved = next_run_is_only_the_request(&mut live, [window].into_iter().collect())
        .await
        .result
        .unwrap();
    assert_eq!(moved.windows[0].rows, initial.windows[0].rows);
}

#[tokio::test]
async fn album_detail_subscription_ignores_an_unread_column() {
    let (db, _temp) = live_db().await;
    super::write_cover(&db, RELEASE_ID, "cover bytes").await;
    let mut live = db.subscribe_album_detail(Some(ALBUM_ID.to_string()));
    live.next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();

    exec(
        &db,
        "UPDATE covers SET source_url = 'https://example.test/cover' WHERE id = ?1",
        &[RELEASE_ID],
    )
    .await;

    next_run_is_only_the_request(&mut live, Some(OTHER_ALBUM_ID.to_string())).await;
}

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db().await;
    exec(
        &db,
        "INSERT INTO albums
         (id, title, is_compilation, created_at) VALUES (?1, 'Other Album', 0, '2026-01-01T00:00:00Z')",
        &[OTHER_ALBUM_ID],
    )
    .await;
    // The request only gives the test a change of its own to make; the query
    // reads the same row whatever it is.
    let mut live = db.inner.handle.subscribe_reconfigurable(0_u8, |_, sql| {
        sql.query_row(
            "SELECT title FROM albums WHERE id = ?1",
            params![ALBUM_ID],
            |row| row.get::<_, String>(0),
        )
        .map_err(CovenError::from)
    });
    assert_eq!(
        live.next()
            .await
            .expect("the query stays open")
            .result
            .unwrap(),
        "Album Title"
    );

    exec(
        &db,
        "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1",
        &[OTHER_ALBUM_ID],
    )
    .await;

    let moved = next_run_is_only_the_request(&mut live, 1).await;
    assert_eq!(moved.result.unwrap(), "Album Title");
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_detail(Some(ALBUM_ID.to_string()));
    assert!(live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .detail
        .is_some());

    exec(&db, "DELETE FROM albums WHERE id = ?1", &[ALBUM_ID]).await;

    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("album deletion wakes album detail")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(deleted.detail.is_none());
}

#[path = "live_query_tests/applied_writes.rs"]
mod applied_writes;
#[path = "live_query_tests/id_sets.rs"]
mod id_sets;
