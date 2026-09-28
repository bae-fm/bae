use super::super::*;

/// An in-memory DB on the real schema with one artist/album/release whose
/// `created_at` is valid, so a test can corrupt one column and prove the
/// mapper rejects it.
fn seeded_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../../../../migrations/001_initial.sql"))
        .unwrap();
    let now = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT INTO artists (id, name, name_key, _updated_at, created_at) VALUES ('6c441836-aef7-4239-8a84-5336c4cce52c', 'Artist Name', 'artist name', ?, ?)",
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
        "INSERT INTO releases (id, album_id, remote, _updated_at, created_at) \
         VALUES ('cccb6034-5922-40d2-8d0b-d94619230882', '9644b84d-94b2-4b3b-863a-d6583931920c', 1, ?, ?)",
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

/// Insert one `release_files` row of the seeded release with the given
/// source-audio layout, codec and bit depth.
fn insert_file(
    conn: &Connection,
    id: &str,
    layout: Option<&str>,
    audio: Option<(&str, i64)>,
) -> coven::rusqlite::Result<usize> {
    let now = "2026-01-01T00:00:00Z";
    let (content_type, duration_ms, sample_rate_hz, bits_per_sample, channels) = match audio {
        Some((content_type, bits)) => (
            Some(content_type),
            Some(1_000),
            Some(44_100),
            Some(bits),
            Some(2),
        ),
        None => (None, None, None, None, None),
    };
    conn.execute(
        "INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, \
         hash, _updated_at, created_at, source_audio_layout, source_audio_content_type, \
         source_audio_duration_ms, source_audio_sample_rate_hz, source_audio_bits_per_sample, \
         source_audio_channels) \
         VALUES (?1, 'cccb6034-5922-40d2-8d0b-d94619230882', ?1, 1, 'audio/flac', 'hash', ?2, ?2, \
         ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            now,
            layout,
            content_type,
            duration_ms,
            sample_rate_hz,
            bits_per_sample,
            channels
        ],
    )
}

/// Audio a release carries while its tracklist leaves it out has no layout;
/// every other audio file has one; a file that is not audio has neither.
#[test]
fn only_audio_has_a_layout_and_audio_left_out_of_the_tracklist_has_none() {
    let conn = seeded_conn();
    insert_file(&conn, "left-out", None, Some(("audio/flac", 16)))
        .expect("audio left out of the tracklist has no layout");
    insert_file(&conn, "track", Some("file"), Some(("audio/flac", 16))).expect("a track's file");
    insert_file(&conn, "disc", Some("cue"), Some(("audio/flac", 16))).expect("a CUE disc image");
    insert_file(&conn, "artwork", None, None).expect("a file that is not audio");
    for (id, layout, audio) in [
        ("layout-without-audio", Some("file"), None),
        ("unknown-layout", Some("disc"), Some(("audio/flac", 16))),
    ] {
        let error = insert_file(&conn, id, layout, audio).expect_err(id);
        assert!(
            error.to_string().contains("CHECK constraint failed"),
            "{id}: {error}"
        );
    }

    let left_out = conn
        .query_row(
            "SELECT * FROM release_files WHERE id = 'left-out'",
            [],
            row_to_file,
        )
        .unwrap();
    let audio = left_out
        .source_audio
        .expect("the facts of carried audio read back");
    assert_eq!(audio.layout, None);
}
