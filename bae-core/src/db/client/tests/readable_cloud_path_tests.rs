use super::super::*;
use super::*;

/// An in-memory DB on the real schema with one artist/album/release, so the
/// connection-level resolvers can look up a release's album id.
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
fn audio_key_omits_source_folder_when_release_has_none() {
    // The seeded release has no source_folder_name (a non-folder import). The
    // stored key is namespace-relative; coven prepends the `release_files`
    // namespace when it reads/writes the blob.
    let conn = seeded_conn();
    let key = resolve_audio_cloud_path(&conn, REL_1, "01 Track Title.flac").unwrap();
    assert_eq!(key, format!("{ALBUM_1}/{REL_1}/01 Track Title.flac"));
}

#[test]
fn audio_key_includes_source_folder_from_the_release_row() {
    let conn = seeded_conn();
    conn.execute(
        "UPDATE releases SET source_folder_name = 'Album Folder [FLAC]' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
        [],
    )
    .unwrap();
    let key = resolve_audio_cloud_path(&conn, REL_1, "01 Track Title.flac").unwrap();
    assert_eq!(
        key,
        format!("{ALBUM_1}/{REL_1}/Album Folder [FLAC]/01 Track Title.flac")
    );
}

#[test]
fn cover_key_is_album_release_and_blob_id() {
    // The blob id rides in the key, so a replaced cover writes a new object
    // rather than overwriting the one it replaces.
    let conn = seeded_conn();
    let key = resolve_cover_cloud_path(&conn, REL_1, BLOB_1, &ContentType::Jpeg).unwrap();
    assert_eq!(key, format!("{ALBUM_1}/{REL_1}/cover-{BLOB_1}.jpg"));
    let replaced = resolve_cover_cloud_path(&conn, REL_1, BLOB_2, &ContentType::Jpeg).unwrap();
    assert_ne!(key, replaced);
}

#[test]
fn artist_key_is_artist_and_blob_id() {
    // Keyed by the artist and its blob id alone -- no DB lookup.
    let key = resolve_artist_cloud_path(ARTIST_1, BLOB_1, &ContentType::Png);
    assert_eq!(key, format!("{ARTIST_1}/artist-{BLOB_1}.png"));
}

#[test]
fn missing_release_is_an_error() {
    // The release row must exist when a blob is keyed; its absence is a
    // broken invariant surfaced as an error, not masked.
    let conn = seeded_conn();
    assert!(resolve_audio_cloud_path(&conn, "no-such-release", "x.flac").is_err());
}
