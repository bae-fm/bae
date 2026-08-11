use super::super::*;

/// An in-memory DB on the real schema with one artist/album/release whose
/// `created_at`/`metadata_source` are valid, so a test can corrupt one
/// column and prove the mapper rejects it.
fn seeded_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../../../migrations/001_initial.sql"))
        .unwrap();
    let now = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('6c441836-aef7-4239-8a84-5336c4cce52c', 'Artist Name', ?, ?)",
        params![now, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
         VALUES ('9644b84d-94b2-4b3b-863a-d6583931920c', 'Album Title', '6c441836-aef7-4239-8a84-5336c4cce52c', 0, ?, ?)",
        params![now, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
         VALUES ('cccb6034-5922-40d2-8d0b-d94619230882', '9644b84d-94b2-4b3b-863a-d6583931920c', 'file_tags', 1, ?, ?)",
        params![now, now],
    )
    .unwrap();
    conn
}

#[test]
fn row_to_release_rejects_malformed_created_at() {
    // A corrupt timestamp must propagate as an error, not panic the mapper.
    let conn = seeded_conn();
    conn.execute(
        "UPDATE releases SET created_at = 'not-a-timestamp' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
        [],
    )
    .unwrap();
    let result = conn.query_row(
        "SELECT * FROM releases WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
        [],
        row_to_release,
    );
    assert!(result.is_err());
}

#[test]
fn row_to_release_rejects_unknown_metadata_source() {
    // An unknown enum string must propagate, not panic via expect.
    let conn = seeded_conn();
    conn.execute(
        "UPDATE releases SET metadata_source = 'bogus' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
        [],
    )
    .unwrap();
    let result = conn.query_row(
        "SELECT * FROM releases WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
        [],
        row_to_release,
    );
    assert!(result.is_err());
}
