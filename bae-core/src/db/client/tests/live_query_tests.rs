use super::super::*;
use super::store_file_helpers::{close_store, copy_store};
use crate::library::LibraryPageWindow;
use coven::SystemClock;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

const ARTIST_ID: &str = "96f0ef68-c284-4f74-b3f6-d9a4b48ee6d1";
const ALBUM_ID: &str = "975b724f-fce7-4fdb-85be-9f57fd9ba496";
const RELEASE_ID: &str = "44751464-1552-44be-8c5a-955cc5b61d12";
const OTHER_ALBUM_ID: &str = "5cf43aa4-8374-4ec4-bf08-239685a62f37";
const IDENTITY_ID: &str = "d9af7374-f7de-4d5b-8f9e-19b250ca2693";
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
    let temp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        temp.path().join("library.db").to_str().unwrap(),
        Arc::new(SystemClock),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|sql| {
        sql.execute_batch(&format!(
            "INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'seed', '2026-01-01T00:00:00Z');
             INSERT INTO albums
               (id, title, artist_id, primary_release_id, is_compilation, _updated_at, created_at)
             VALUES ('{ALBUM_ID}', 'Album Title', '{ARTIST_ID}', '{RELEASE_ID}', 0, 'seed', '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
             VALUES ('{RELEASE_ID}', '{ALBUM_ID}', 'file_tags', 1, 'seed', '2026-01-01T00:00:00Z');"
        ))
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, temp)
}

async fn browsable_sync_db(path: &std::path::Path, device_id: &str) -> Database {
    let library_dir = coven::StoreDir::new(path.parent().unwrap());
    let mut config = coven::Config::with_defaults(
        "test-library".to_string(),
        device_id.to_string(),
        "Test Library".to_string(),
    );
    config.cloud_home.storage = coven::HomeStorage::Browsable;
    crate::config::install_test_keyring();
    Database::open(
        library_dir,
        config,
        Arc::new(SystemClock),
        Arc::new(coven::UuidProvider),
        crate::sync::synced_tables(),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn album_page_subscription_delivers_rows_count_and_cover_versions() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_page(&[], 0, 50);

    let initial = live.next().await.unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.rows[0].title, "Album Title");
    assert!(initial.cover_versions.is_empty());

    let cover_hash = crate::util::fs::hash_bytes(b"cover fixture");
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO covers
             (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
             VALUES (?1, ?2, 'image/jpeg', 12, 'file_tags', ?3, 'cover-v1', '2026-01-01T00:00:00Z')",
            params![
                RELEASE_ID,
                "bd5c1f6c-3b6e-4d16-9f0a-2c1d5f61a0aa",
                cover_hash
            ],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("cover write wakes album page")
        .unwrap();
    assert_eq!(
        updated.cover_versions.get(RELEASE_ID).map(String::as_str),
        Some("cover-v1")
    );
}

#[tokio::test]
async fn album_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db().await;
    let sort = [AlbumSortCriterion {
        field: AlbumSortField::DateAdded,
        direction: SortDirection::Ascending,
    }];
    let mut live = db.subscribe_album_browse(&sort, BTreeSet::new());
    let requests = live.requests();
    let initial_event = live.next().await;
    assert_eq!(initial_event.revision().get(), 0);
    let initial = initial_event.into_result().unwrap();
    assert_eq!(initial.total_count, 1);
    assert!(initial.windows.is_empty());

    db.call(|sql| {
        sql.execute_batch(&format!(
            "INSERT INTO albums
               (id, title, artist_id, primary_release_id, is_compilation, _updated_at, created_at)
             VALUES ('{OTHER_ALBUM_ID}', 'Album Title Second', '{ARTIST_ID}', '{OTHER_RELEASE_ID}', 0, 'album-v1', '2026-01-02T00:00:00Z');
             INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
             VALUES ('{OTHER_RELEASE_ID}', '{OTHER_ALBUM_ID}', 'file_tags', 1, 'release-v1', '2026-01-02T00:00:00Z');"
        ))
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let inserted = live.next().await.into_result().unwrap();
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
    let pages_event = live.next().await;
    assert_eq!(pages_event.request(), &requested);
    assert_eq!(pages_event.revision(), pages_revision);
    let pages = pages_event.into_result().unwrap();
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
    let second_page_event = live.next().await;
    assert_eq!(second_page_event.request(), &second_page);
    assert_eq!(second_page_event.revision(), second_page_revision);
    let second_page_initial = second_page_event.into_result().unwrap();
    assert_eq!(second_page_initial.windows[0].rows[0].id, OTHER_ALBUM_ID);

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET title = 'Album Title Renamed' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let renamed_event = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first album metadata wakes album browse");
    assert_eq!(renamed_event.revision(), second_page_revision);
    let renamed = renamed_event.into_result().unwrap();
    assert_eq!(renamed.windows[0].rows[0].title, "Album Title Renamed");

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET created_at = '2025-12-01T00:00:00Z' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let reordered = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first album ordering field wakes album browse")
        .into_result()
        .unwrap();
    assert_eq!(reordered.windows[0].rows[0].id, ALBUM_ID);

    let first_page = [first_window].into_iter().collect::<BTreeSet<_>>();
    requests.set(first_page).unwrap();
    let reordered_first = live.next().await.into_result().unwrap();
    assert_eq!(reordered_first.windows[0].rows[0].id, OTHER_ALBUM_ID);

    requests
        .set([second_window].into_iter().collect::<BTreeSet<_>>())
        .unwrap();
    let hidden_other_album = live.next().await.into_result().unwrap();
    assert_eq!(hidden_other_album.windows[0].rows[0].id, ALBUM_ID);

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET title = 'Album Title Hidden' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    // The edited album is outside the requested window, so the rerun
    // produces the page already delivered and coven withholds it.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err(),
        "unrequested album metadata leaves the delivered page unchanged"
    );

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET _updated_at = 'unread-column-write' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );

    let cover_hash = crate::util::fs::hash_bytes(b"other cover fixture");
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO covers
             (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
             VALUES (?1, ?2, 'image/jpeg', 19, 'file_tags', ?3, 'cover-v1', '2026-01-02T00:00:00Z')",
            params![
                OTHER_RELEASE_ID,
                "96f3c15a-b99d-4395-81e5-2c32bb7a9c75",
                cover_hash
            ],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let covered = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first album cover wakes album browse")
        .into_result()
        .unwrap();
    assert_eq!(
        covered
            .cover_versions
            .get(OTHER_RELEASE_ID)
            .map(String::as_str),
        Some("cover-v1")
    );

    db.call(|sql| {
        sql.execute(
            "UPDATE covers SET _updated_at = 'cover-v2' WHERE id = ?1",
            params![OTHER_RELEASE_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let cover_updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first album cover version wakes album browse")
        .into_result()
        .unwrap();
    assert_eq!(
        cover_updated
            .cover_versions
            .get(OTHER_RELEASE_ID)
            .map(String::as_str),
        Some("cover-v2")
    );

    db.call(|sql| {
        sql.execute("DELETE FROM albums WHERE id = ?1", params![OTHER_ALBUM_ID])
            .map(|_| ())
            .map_err(DbError::from)
    })
    .await
    .unwrap();
    let deleted = live.next().await.into_result().unwrap();
    assert_eq!(deleted.total_count, 1);
    assert!(deleted.windows[0].rows.is_empty());
}

#[tokio::test]
async fn composer_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db().await;
    db.call(|sql| {
        sql.execute_batch(&format!(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES
               ('{COMPOSER_ID}', 'Composer Name First', 'composer-v1', '2026-01-01T00:00:00Z'),
               ('{OTHER_COMPOSER_ID}', 'Composer Name Second', 'composer-v1', '2026-01-02T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, _updated_at, created_at) VALUES
               ('{COMPOSER_WORK_ID}', 'Work Title First', 'work', 'work-first', 'work-v1', '2026-01-01T00:00:00Z'),
               ('{OTHER_COMPOSER_WORK_ID}', 'Work Title Second', 'work', 'work-second', 'work-v1', '2026-01-02T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at) VALUES
               ('{COMPOSER_LINK_ID}', '{COMPOSER_WORK_ID}', '{COMPOSER_ID}', 0, 'file_tags', 'link-v1', '2026-01-01T00:00:00Z'),
               ('{OTHER_COMPOSER_LINK_ID}', '{OTHER_COMPOSER_WORK_ID}', '{OTHER_COMPOSER_ID}', 0, 'file_tags', 'link-v1', '2026-01-02T00:00:00Z');"
        ))
        .map_err(DbError::from)
    })
    .await
    .unwrap();
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
    let initial_event = live.next().await;
    assert_eq!(initial_event.request(), &initial_request);
    assert_eq!(initial_event.revision().get(), 0);
    let initial = initial_event.into_result().unwrap();
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
    let both_event = live.next().await;
    assert_eq!(both_event.revision(), both_pages_revision);
    let both = both_event.into_result().unwrap();
    assert_eq!(both.windows.len(), 2);
    assert_eq!(both.windows[0].rows[0].artist.id, OTHER_COMPOSER_ID);
    assert_eq!(both.windows[1].rows[0].artist.id, COMPOSER_ID);

    db.call(|sql| {
        sql.execute(
            "UPDATE artists SET name = 'Composer Name Renamed' WHERE id = ?1",
            params![OTHER_COMPOSER_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let renamed_event = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first composer metadata wakes composer browse");
    assert_eq!(renamed_event.revision(), both_pages_revision);
    let renamed = renamed_event.into_result().unwrap();
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
    let hidden_other_composer = live.next().await.into_result().unwrap();
    assert_eq!(
        hidden_other_composer.windows[0].rows[0].artist.id,
        COMPOSER_ID
    );

    db.call(|sql| {
        sql.execute(
            "UPDATE artists SET sort_name = 'Composer Sort Hidden' WHERE id = ?1",
            params![OTHER_COMPOSER_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    // The edited composer is outside the requested window, so the rerun
    // produces the page already delivered and coven withholds it.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err(),
        "unrequested composer metadata leaves the delivered page unchanged"
    );

    let image_hash = crate::util::fs::hash_bytes(b"other artist fixture");
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO artist_images
             (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
             VALUES (?1, ?2, 'image/jpeg', 20, 'file_tags', ?3, 'image-v1', '2026-01-02T00:00:00Z')",
            params![
                OTHER_COMPOSER_ID,
                "19af4b72-9f57-4110-bc92-b72735b7b4ad",
                image_hash
            ],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let imaged = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first composer image wakes composer browse")
        .into_result()
        .unwrap();
    assert_eq!(
        imaged
            .image_versions
            .get(OTHER_COMPOSER_ID)
            .map(String::as_str),
        Some("image-v1")
    );

    db.call(|sql| {
        sql.execute(
            "UPDATE artist_images SET _updated_at = 'image-v2' WHERE id = ?1",
            params![OTHER_COMPOSER_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let image_updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("non-first composer image version wakes composer browse")
        .into_result()
        .unwrap();
    assert_eq!(
        image_updated
            .image_versions
            .get(OTHER_COMPOSER_ID)
            .map(String::as_str),
        Some("image-v2")
    );

    requests.set(BTreeSet::new()).unwrap();
    let count_only = live.next().await.into_result().unwrap();
    assert_eq!(count_only.total_count, 2);
    assert!(count_only.windows.is_empty());

    db.call(|sql| {
        sql.execute_batch(&format!(
            "INSERT INTO artists (id, name, _updated_at, created_at)
             VALUES ('{INSERTED_COMPOSER_ID}', 'Composer Name Inserted', 'composer-v1', '2026-01-03T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, _updated_at, created_at)
             VALUES ('{INSERTED_COMPOSER_WORK_ID}', 'Work Title Inserted', 'work', 'work-inserted', 'work-v1', '2026-01-03T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
             VALUES ('{INSERTED_COMPOSER_LINK_ID}', '{INSERTED_COMPOSER_WORK_ID}', '{INSERTED_COMPOSER_ID}', 0, 'file_tags', 'link-v1', '2026-01-03T00:00:00Z');"
        ))
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let inserted = live.next().await.into_result().unwrap();
    assert_eq!(inserted.total_count, 3);
    assert!(inserted.windows.is_empty());

    db.call(|sql| {
        sql.execute(
            "DELETE FROM work_artists WHERE id = ?1",
            params![INSERTED_COMPOSER_LINK_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let deleted = live.next().await.into_result().unwrap();
    assert_eq!(deleted.total_count, 2);
    assert!(deleted.windows.is_empty());
}

#[tokio::test]
async fn album_page_subscription_ignores_an_unread_table() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_page(&[], 0, 50);
    live.next().await.unwrap();

    db.call(|sql| {
        sql.execute(
            "INSERT INTO playback_state
             (id, source, shuffled, manual, repeat, current_track_id, position_ms, volume, is_muted)
             VALUES ('current', NULL, NULL, '[]', 'off', NULL, 0, 1.0, 0)",
            [],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn album_detail_subscription_ignores_an_unread_column() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_detail(ALBUM_ID);
    live.next().await.unwrap();

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET _updated_at = 'unread-column-write' WHERE id = ?1",
            params![ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db().await;
    db.call(|sql| {
        sql.execute(
            "INSERT INTO albums
             (id, title, artist_id, is_compilation, _updated_at, created_at)
             VALUES (?1, 'Other Album', ?2, 0, 'seed', '2026-01-01T00:00:00Z')",
            params![OTHER_ALBUM_ID, ARTIST_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    let mut live = db.inner.handle.subscribe(|sql| {
        sql.query_row(
            "SELECT title FROM albums WHERE id = ?1",
            params![ALBUM_ID],
            |row| row.get::<_, String>(0),
        )
        .map_err(CovenError::from)
    });
    assert_eq!(live.next().await.unwrap(), "Album Title");

    db.call(|sql| {
        sql.execute(
            "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1",
            params![OTHER_ALBUM_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(100), live.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db().await;
    let mut live = db.subscribe_album_detail(ALBUM_ID);
    assert!(live.next().await.unwrap().detail.is_some());

    db.call(|sql| {
        sql.execute("DELETE FROM albums WHERE id = ?1", params![ALBUM_ID])
            .map(|_| ())
            .map_err(DbError::from)
    })
    .await
    .unwrap();

    let deleted = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("album deletion wakes album detail")
        .unwrap();
    assert!(deleted.detail.is_none());
}

#[tokio::test]
async fn release_library_status_subscription_delivers_identity_changes() {
    let (db, _temp) = live_db().await;
    let check = LibraryCheck {
        release_id: "source-release-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("source-group-1".to_string()),
    };
    let mut live = db.subscribe_release_library_status(check);

    let initial = live.next().await.unwrap();
    assert!(!initial.release_in_library);
    assert!(!initial.album_in_library);

    db.call(|sql| {
        sql.execute(
            "INSERT INTO release_identities
             (id, release_id, source, source_release_id, source_group_id, _updated_at, created_at)
             VALUES (?1, ?2, 'musicbrainz', 'source-release-1', 'source-group-1', 'identity-v1', '2026-01-01T00:00:00Z')",
            params![IDENTITY_ID, RELEASE_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let updated = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("identity write wakes release library status")
        .unwrap();
    assert!(updated.release_in_library);
    assert!(updated.album_in_library);
    assert_eq!(updated.album_id.as_deref(), Some(ALBUM_ID));
}

/// The import list's request carries the view and the windows, so every test
/// below states both.
pub(super) fn list_request(
    tab: crate::import::TriageTab,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::import::ImportListRequest {
    crate::import::ImportListRequest {
        view: crate::import::ImportListView {
            tab,
            ..crate::import::ImportListView::default()
        },
        windows: windows
            .into_iter()
            .map(|(offset, limit)| crate::library::LibraryPageWindow { offset, limit })
            .collect(),
        automatic_identification_enabled: true,
        runtime_facts: Default::default(),
        upload_standing: Default::default(),
    }
}

pub(super) fn scan_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ReleaseFileScope, ScanItem, ScannedFile,
    };
    ScanItem::Valid(crate::import::FolderCandidate {
        path: format!("{root}/{name}").into(),
        file_root: format!("{root}/{name}").into(),
        name: name.to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: ScannedFile::new(
                    format!("{root}/{name}/01.flac").into(),
                    "01.flac".to_string(),
                    1_000,
                ),
                role: FileRole::Audio,
            }],
            format_label: "FLAC".to_string(),
        },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: name.to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    })
}

pub(super) fn candidate_names(projection: &crate::import::ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row.folder_name.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn import_list_moves_a_row_to_done_when_its_content_hash_is_imported() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::folder_registry::host_root("/music");
    let item = scan_candidate(root, "release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 50)]));
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(initial.total_count, 1);
    assert_eq!(initial.summary.counts.pending, 1);
    assert_eq!(initial.summary.counts.done, 0);

    db.call(move |sql| {
        sql.execute(
            "UPDATE releases SET content_hash = ?1 WHERE id = ?2",
            params![content_hash, RELEASE_ID],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let imported = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the imported release wakes the list")
        .into_result()
        .unwrap();
    assert_eq!(imported.total_count, 0, "Pending no longer holds the row");
    assert_eq!(imported.summary.counts.done, 1);
}

/// Moving the window is a request change, not a commit: the query reruns and
/// says so without anything having been written.
#[tokio::test]
async fn import_list_moving_the_window_reruns_without_a_commit() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::folder_registry::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let live = db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 1)]));
    let requests = live.requests();
    let mut live = live;
    let initial = live.next().await;
    assert_eq!(
        initial.cause(),
        coven::ReconfigurableLiveQueryCause::Initial
    );
    assert_eq!(
        candidate_names(&initial.into_result().unwrap()),
        vec!["first".to_string()]
    );

    requests
        .set(list_request(crate::import::TriageTab::Pending, [(1, 1)]))
        .unwrap();
    let moved = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .expect("the window change reruns the query");
    assert_eq!(
        moved.cause(),
        coven::ReconfigurableLiveQueryCause::RequestChanged
    );
    assert_eq!(
        candidate_names(&moved.into_result().unwrap()),
        vec!["second".to_string()]
    );
}

/// A commit that touches a column the list does not read leaves the projection
/// equal, and coven withholds it: the tab does not re-render for a write it
/// cannot show.
#[tokio::test]
async fn import_list_withholds_a_commit_that_changes_nothing_it_reads() {
    let (db, _temp) = live_db().await;
    let root = &crate::import::folder_registry::host_root("/music");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["first", "second"] {
        db.save_folder_scan_item(root, generation, &scan_candidate(root, name))
            .await
            .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let mut live =
        db.subscribe_import_list(list_request(crate::import::TriageTab::Pending, [(0, 1)]));
    let initial = live.next().await.into_result().unwrap();
    assert_eq!(candidate_names(&initial), vec!["first".to_string()]);

    let second = format!("{root}/second");
    db.call(move |sql| {
        sql.execute(
            "UPDATE scan_candidate SET format_label = 'ALAC' WHERE path = ?1",
            params![second],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(500), live.next())
            .await
            .is_err(),
        "a commit the list reads nothing from delivers no value"
    );
}

#[tokio::test]
async fn album_page_subscription_delivers_a_write_materialized_by_sync() {
    let seed_dir = tempfile::TempDir::new().unwrap();
    let writer_dir = tempfile::TempDir::new().unwrap();
    let reader_dir = tempfile::TempDir::new().unwrap();
    let seed = browsable_sync_db(&seed_dir.path().join("library.db"), "test-device").await;
    let home = Arc::new(coven::InMemoryCloudHome::new());
    seed.establish_test_identity().unwrap();
    seed.connect_sync_with_test_home(home.clone(), coven::CloudCipher::Plaintext)
        .await
        .unwrap();
    seed.disconnect_cloud_home().await.unwrap();
    close_store(seed);
    copy_store(seed_dir.path(), writer_dir.path());
    copy_store(seed_dir.path(), reader_dir.path());

    let writer = browsable_sync_db(&writer_dir.path().join("library.db"), "test-device").await;
    let reader = browsable_sync_db(&reader_dir.path().join("library.db"), "test-device").await;
    writer
        .connect_sync_with_test_home(home.clone(), coven::CloudCipher::Plaintext)
        .await
        .unwrap();
    reader
        .connect_sync_with_test_home(home, coven::CloudCipher::Plaintext)
        .await
        .unwrap();

    let mut live = reader.subscribe_album_page(&[], 0, 50);
    assert_eq!(live.next().await.unwrap().total_count, 0);

    writer
        .call(|sql| {
            sql.execute_batch(&format!(
                "INSERT INTO artists (id, name, _updated_at, created_at)
                 VALUES ('{ARTIST_ID}', 'Artist Name', 'remote', '2026-01-01T00:00:00Z');
                 INSERT INTO albums
                   (id, title, artist_id, primary_release_id, is_compilation, _updated_at, created_at)
                 VALUES ('{ALBUM_ID}', 'Synced Album', '{ARTIST_ID}', '{RELEASE_ID}', 0, 'remote', '2026-01-01T00:00:00Z');
                 INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                 VALUES ('{RELEASE_ID}', '{ALBUM_ID}', 'file_tags', 1, 'remote', '2026-01-01T00:00:00Z');"
            ))
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    writer.sync_now();

    let updated = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            reader.sync_now();
            if let Ok(Ok(value)) =
                tokio::time::timeout(Duration::from_millis(250), live.next()).await
            {
                if value.total_count == 1 {
                    break value;
                }
            }
        }
    })
    .await
    .expect("reader materializes the writer's synced album");
    assert_eq!(updated.rows[0].title, "Synced Album");
}
