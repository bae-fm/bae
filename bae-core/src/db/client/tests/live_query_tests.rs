use super::super::*;
use super::{exec, exec_batch};
use crate::db::LibraryPageWindow;
use std::collections::BTreeSet;
use std::time::Duration;

pub(crate) const ARTIST_ID: &str = "96f0ef68-c284-4f74-b3f6-d9a4b48ee6d1";
pub(crate) const ALBUM_ID: &str = "975b724f-fce7-4fdb-85be-9f57fd9ba496";
pub(crate) const RELEASE_ID: &str = "44751464-1552-44be-8c5a-955cc5b61d12";
pub(crate) const OTHER_ALBUM_ID: &str = "5cf43aa4-8374-4ec4-bf08-239685a62f37";
pub(crate) const IDENTITY_ID: &str = "d9af7374-f7de-4d5b-8f9e-19b250ca2693";
pub(crate) const OTHER_RELEASE_ID: &str = "79764470-a937-42fd-bbd4-fce67651c72e";
pub(crate) const COMPOSER_ID: &str = "735c571d-5dcf-4a52-af70-f080f6a82a2d";
pub(crate) const OTHER_COMPOSER_ID: &str = "041df287-6a78-4db4-981d-70eb24bad7ec";
pub(crate) const INSERTED_COMPOSER_ID: &str = "7cc478ba-dd73-4b9f-9739-a1524088800e";
pub(crate) const COMPOSER_WORK_ID: &str = "eb9cf137-3917-408a-9e4f-670c5f39051d";
pub(crate) const OTHER_COMPOSER_WORK_ID: &str = "9958dcbd-ebac-42b9-9b6e-77bc421ac32b";
pub(crate) const INSERTED_COMPOSER_WORK_ID: &str = "e1aa84de-4495-499c-92f8-9ff6ccdc59f5";
pub(crate) const COMPOSER_LINK_ID: &str = "8621fd15-044a-44dd-ae70-dd6de30e35da";
pub(crate) const OTHER_COMPOSER_LINK_ID: &str = "49a10300-3036-4301-9e1b-a50a76a46032";
pub(crate) const INSERTED_COMPOSER_LINK_ID: &str = "9168d755-3ed6-446c-a0ec-5f4039e18a6f";

pub(crate) async fn live_db() -> (Database, tempfile::TempDir) {
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
pub(crate) async fn write_artist_image(db: &Database, artist: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    db.write_library_image(
        &DbLibraryImage {
            id: artist.into(),
            image_type: LibraryImageType::Artist,
            content_type: ContentType::Jpeg,
            width: None,
            height: None,
            created_at: super::fixed_now(),
        },
        bytes,
    )
    .await
    .unwrap();
    super::stored_image_version(db, crate::db::synced::ARTIST_IMAGES_NAMESPACE, artist).await
}

/// Move `live` to `request` and return the event answering it, asserting the
/// move alone caused that run. Every commit since the query's last value is
/// read into the same run, so one touching anything the query reads makes the
/// cause `RequestAndDatabaseChanged`.
pub(crate) async fn next_run_is_only_the_request<Request, Value>(
    live: &mut coven::ReconfigurableLiveQuery<Request, Value>,
    request: Request,
) -> coven::ReconfigurableLiveQueryEvent<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    let revision = live.requests().set(request).expect("the subscription is open");
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

/// The first page of a list, as the one window a browse query reads.
pub(crate) fn first_page() -> BTreeSet<LibraryPageWindow> {
    [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect()
}

#[tokio::test]
pub(crate) async fn single_table_subscription_ignores_a_different_primary_key() {
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
        sql.query_row("SELECT title FROM albums WHERE id = ?1", params![ALBUM_ID], |row| {
            row.get::<_, String>(0)
        })
        .map_err(CovenError::from)
    });
    assert_eq!(live.next().await.expect("the query stays open").result.unwrap(), "Album Title");

    exec(&db, "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1", &[OTHER_ALBUM_ID])
        .await;

    let moved = next_run_is_only_the_request(&mut live, 1).await;
    assert_eq!(moved.result.unwrap(), "Album Title");
}

#[path = "live_query_tests/id_sets.rs"]
mod id_sets;
