//! A library of one artist, album and release, the ids live-query tests
//! add rows under, and how a test checks what moved a live query.

use super::exec_batch;
use crate::{Database, DbLibraryImage, LibraryImageType, LibraryPageWindow};
use bae_util::content_type::ContentType;
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

pub const ARTIST_ID: &str = "96f0ef68-c284-4f74-b3f6-d9a4b48ee6d1";
pub const ALBUM_ID: &str = "975b724f-fce7-4fdb-85be-9f57fd9ba496";
pub const RELEASE_ID: &str = "44751464-1552-44be-8c5a-955cc5b61d12";
pub const OTHER_ALBUM_ID: &str = "5cf43aa4-8374-4ec4-bf08-239685a62f37";
pub const IDENTITY_ID: &str = "d9af7374-f7de-4d5b-8f9e-19b250ca2693";
pub const OTHER_RELEASE_ID: &str = "79764470-a937-42fd-bbd4-fce67651c72e";
pub const COMPOSER_ID: &str = "735c571d-5dcf-4a52-af70-f080f6a82a2d";
pub const OTHER_COMPOSER_ID: &str = "041df287-6a78-4db4-981d-70eb24bad7ec";
pub const INSERTED_COMPOSER_ID: &str = "7cc478ba-dd73-4b9f-9739-a1524088800e";
pub const COMPOSER_WORK_ID: &str = "eb9cf137-3917-408a-9e4f-670c5f39051d";
pub const OTHER_COMPOSER_WORK_ID: &str = "9958dcbd-ebac-42b9-9b6e-77bc421ac32b";
pub const INSERTED_COMPOSER_WORK_ID: &str = "e1aa84de-4495-499c-92f8-9ff6ccdc59f5";
pub const COMPOSER_LINK_ID: &str = "8621fd15-044a-44dd-ae70-dd6de30e35da";
pub const OTHER_COMPOSER_LINK_ID: &str = "49a10300-3036-4301-9e1b-a50a76a46032";
pub const INSERTED_COMPOSER_LINK_ID: &str = "9168d755-3ed6-446c-a0ec-5f4039e18a6f";

pub async fn live_db(schema: crate::Schema) -> (Database, tempfile::TempDir) {
    let (db, temp) = super::temp_db(schema).await;
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
pub async fn write_artist_image(db: &Database, artist: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    db.write_library_image(
        &DbLibraryImage {
            id: artist.into(),
            image_type: LibraryImageType::Artist,
            content_type: ContentType::Jpeg,
        },
        bytes,
    )
    .await
    .unwrap();
    super::stored_image_version(db, crate::synced::ARTIST_IMAGES_NAMESPACE, artist).await
}

/// Move `live` to `request` and return the event answering it, asserting the
/// move alone caused that run. Every commit since the query's last value is
/// read into the same run, so one touching anything the query reads makes the
/// cause `RequestAndDatabaseChanged`.
pub async fn next_run_is_only_the_request<Request, Value>(
    live: &mut coven::ReconfigurableLiveQuery<Request, Value>,
    request: Request,
) -> coven::ReconfigurableLiveQueryEvent<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Clone + PartialEq + Send + 'static,
{
    let revision = live.requests().set(request).expect("the subscription is open");
    let event = tokio::time::timeout(HANG_GUARD, live.next())
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
pub fn first_page() -> BTreeSet<LibraryPageWindow> {
    [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect()
}
