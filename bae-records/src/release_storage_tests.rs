use crate::test_fixtures::storage_listing::*;

/// The releases uploading are read in queue order from an index of their
/// storage rows that holds every column read, not from coven's queue grouped
/// by release.
#[tokio::test]
async fn the_uploading_releases_read_their_storage_in_queue_order() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    assert_eq!(
        plan(
            &db,
            format!(
                "SELECT release_id FROM ({}) ORDER BY position",
                crate::records::query::UPLOADING_RELEASES
            )
        )
        .await,
        vec!["SEARCH uploading USING COVERING INDEX idx_release_storage_uploading (first_queued_position>?)".to_string()]
    );
}

/// `release`'s storage row: its file count, total size, `stored`, pinned
/// and queued counts, and first queued place.
fn storage_row(
    conn: &rusqlite::Connection,
    release: &str,
) -> (i64, i64, i64, i64, i64, Option<i64>) {
    conn.query_row(
        "SELECT file_count, total_size, stored, pinned_count, queued_count, first_queued_position \
         FROM release_storage WHERE release_id = ?1",
        [release],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    )
    .unwrap()
}

/// A release's file, its coven record and its pin arriving before the
/// release, as a write applied from another device can hold them, are
/// counted in the release's storage row as it arrives; the file joining the
/// upload queue after moves the row's queued count and first place.
#[test]
fn a_release_s_storage_counts_files_that_arrived_before_it() {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::test_fixtures::bare_library::open_bare_library(&conn);
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO coven_files (id, audience, size, hash, coordinates, namespace) VALUES
             ('cover-file', '', 7, X'00', '', 'release_files');
         INSERT INTO release_files (id, release_id, original_filename, content_type, created_at, file) VALUES
             ('cover', 'af4421de-5174-45a9-81fc-74110bc5f85d', 'cover.jpg', 'image/jpeg', '2026-01-01T00:00:00Z', 'cover-file');
         INSERT INTO coven_pins (file) VALUES ('cover-file');
         INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('dab6efe2-9b5c-4034-b95e-73cf49f54b9a', 'Album Title B', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('af4421de-5174-45a9-81fc-74110bc5f85d', 'dab6efe2-9b5c-4034-b95e-73cf49f54b9a', '2026-01-01T00:00:00Z', 0);",
    )
    .unwrap();
    assert_eq!(storage_row(&conn, RELEASE_LATE), (1, 7, 1, 1, 0, None));

    conn.execute_batch("INSERT INTO coven_upload_queue (file, position) VALUES ('cover-file', 3);")
        .unwrap();
    assert_eq!(storage_row(&conn, RELEASE_LATE), (1, 7, 1, 1, 1, Some(3)));
}
