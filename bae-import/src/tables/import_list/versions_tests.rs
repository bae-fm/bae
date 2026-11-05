//! The versions of the tables the import list's order reads, moved by the
//! writes that move those tables' rows.

/// `source`'s version in `import_list_versions`.
fn version(conn: &rusqlite::Connection, source: &str) -> i64 {
    conn.query_row("SELECT version FROM import_list_versions WHERE source = ?1", [source], |row| {
        row.get(0)
    })
    .unwrap()
}

/// A release's file joining the upload queue moves where Done runs the
/// release, so it moves the version of the releases' storage, by which a
/// list's read knows its last read's rows may sit elsewhere now.
#[test]
fn a_release_s_place_in_the_upload_queue_moving_moves_the_list_s_version() {
    let conn = bae_store::schema::bare_library_connection(crate::TEST_SCHEMA);
    conn.execute_batch(
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0);
         INSERT INTO coven_files (id, audience, size, hash, coordinates, namespace) VALUES
             ('cover-file', '', 7, X'00', '', 'release_files');
         INSERT INTO release_files (id, release_id, original_filename, content_type, created_at, file) VALUES
             ('cover', 'd439555e-9883-4037-a580-48ea33c2c57b', 'cover.jpg', 'image/jpeg', '2026-01-01T00:00:00Z', 'cover-file');",
    )
    .unwrap();
    let before = version(&conn, "release_storage");

    conn.execute_batch("INSERT INTO coven_upload_queue (file, position) VALUES ('cover-file', 3);")
        .unwrap();
    assert!(version(&conn, "release_storage") > before);
}
