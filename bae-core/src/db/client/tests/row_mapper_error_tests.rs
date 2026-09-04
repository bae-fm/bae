use super::super::*;

/// An in-memory DB on the real schema with one artist/album/release whose
/// `created_at` and provenance are valid, so a test can corrupt one
/// column and prove the mapper rejects it.
fn seeded_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../../../migrations/001_initial.sql"))
        .unwrap();
    conn.execute_batch(include_str!(
        "../../../../migrations/003_metadata_drafts_and_provenance.sql"
    ))
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
fn row_to_release_reads_every_provenance_shape() {
    let conn = seeded_conn();
    let now = "2026-01-01T00:00:00Z";
    for (id, source, source_release_id) in [
        ("cccb6034-5922-40d2-8d0b-d94619230883", "none", None),
        (
            "cccb6034-5922-40d2-8d0b-d94619230884",
            "musicbrainz",
            Some("mb-release"),
        ),
        (
            "cccb6034-5922-40d2-8d0b-d94619230885",
            "discogs",
            Some("discogs-release"),
        ),
    ] {
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, metadata_source_release_id, remote, _updated_at, created_at) \
             VALUES (?, '9644b84d-94b2-4b3b-863a-d6583931920c', ?, ?, 1, ?, ?)",
            params![id, source, source_release_id, now, now],
        )
        .unwrap();
    }

    let read = |id: &str| {
        conn.query_row(
            "SELECT * FROM releases WHERE id = ?",
            params![id],
            row_to_release,
        )
        .unwrap()
        .metadata_provenance
    };
    assert_eq!(
        read("cccb6034-5922-40d2-8d0b-d94619230882"),
        Some(crate::import::MetadataProvenance::FileTags)
    );
    assert_eq!(read("cccb6034-5922-40d2-8d0b-d94619230883"), None);
    assert_eq!(
        read("cccb6034-5922-40d2-8d0b-d94619230884"),
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-release".to_string(),
            partners: vec![],
        })
    );
    assert_eq!(
        read("cccb6034-5922-40d2-8d0b-d94619230885"),
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::Discogs,
            release_id: "discogs-release".to_string(),
            partners: vec![],
        })
    );
}

#[test]
fn releases_reject_invalid_provenance_column_pairs() {
    let conn = seeded_conn();
    for (source, release_id) in [
        (None, None),
        (None, Some("source-release")),
        (Some("none"), Some("source-release")),
        (Some("file_tags"), Some("source-release")),
        (Some("musicbrainz"), None),
        (Some("discogs"), None),
        (Some("bogus"), None),
        (Some("bogus"), Some("source-release")),
    ] {
        let result = conn.execute(
            "UPDATE releases SET metadata_source = ?, metadata_source_release_id = ? \
             WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            params![source, release_id],
        );
        assert!(
            result.is_err(),
            "invalid provenance pair source={source:?}, release_id={release_id:?} was stored"
        );
    }
}
