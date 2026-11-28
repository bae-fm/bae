use crate::records::release_with_files::ReleaseFileByIdParams;
use crate::records::test_statements::*;
use crate::records::*;
use bae_store::test_fixtures::bare_statements::{row, run};
use rusqlite::Connection;

const ALBUM: &str = "9644b84d-94b2-4b3b-863a-d6583931920c";
const RELEASE: &str = "cccb6034-5922-40d2-8d0b-d94619230882";
const NOW: &str = "2026-01-01T00:00:00Z";

/// An in-memory DB on the real schema with one album and release whose
/// `created_at` is valid, so a test can corrupt one column and prove the
/// mapper rejects it.
fn seeded_conn() -> Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&conn);
    run(&conn, &PlantAlbumParams { id: ALBUM, title: "Album Title", created_at: NOW }).unwrap();
    run(&conn, &PlantReleaseParams { id: RELEASE, album_id: ALBUM, created_at: NOW }).unwrap();
    conn
}

#[test]
fn a_release_with_a_malformed_created_at_is_unreadable() {
    // A corrupt timestamp must propagate as an error, not panic the reader.
    let conn = seeded_conn();
    run(&conn, &DamageReleaseCreatedAtParams { id: RELEASE, created_at: "not-a-timestamp" })
        .unwrap();
    let statement = OneReleaseParams { release: RELEASE.to_string() };
    let read = row(&conn, &statement).unwrap();
    assert!(read.held.release().is_err());
}

/// The release file `id`, as [`ReleaseFileById`] reads it.
fn read_file(conn: &Connection, id: &str) -> rusqlite::Result<DbFile> {
    let read = row(conn, &ReleaseFileByIdParams { file: id.to_string() })?;
    Ok(read.record.into_file().expect("the stored file decodes"))
}

/// Insert one `release_files` row of the seeded release with the given
/// source-audio layout, codec and bit depth.
fn insert_file(
    conn: &Connection,
    id: &str,
    layout: Option<&str>,
    audio: Option<(&str, i64)>,
) -> rusqlite::Result<usize> {
    let (content_type, container, duration_ms, sample_rate_hz, bits_per_sample, channels) =
        match audio {
            Some((content_type, bits)) => {
                (Some(content_type), Some("flac"), Some(1_000), Some(44_100), Some(bits), Some(2))
            }
            None => (None, None, None, None, None, None),
        };
    // The record of the coven file the row references, as coven writes it: a
    // file only device 1 holds.
    let file = bae_util::test_ids::test_uuid(id);
    run(conn, &PlantCovenFileParams { id: &file })?;
    run(
        conn,
        &PlantReleaseFileParams {
            id,
            release_id: RELEASE,
            content_type: "audio/flac",
            file: &file,
            role: if audio.is_some() { "audio" } else { "other" },
            document_kind: None,
            source_audio_layout: layout,
            source_audio_content_type: content_type,
            source_audio_container: container,
            source_audio_duration_ms: duration_ms,
            source_audio_sample_rate_hz: sample_rate_hz,
            source_audio_bits_per_sample: bits_per_sample,
            source_audio_channels: channels,
            created_at: NOW,
        },
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
        assert!(error.to_string().contains("CHECK constraint failed"), "{id}: {error}");
    }

    let left_out = read_file(&conn, "left-out").unwrap();
    let audio = left_out.source_audio.expect("the facts of carried audio read back");
    assert_eq!(audio.layout, None);
}

/// Insert one non-audio `release_files` row of the seeded release in `role`,
/// with `document_kind`.
fn insert_carried_file(
    conn: &Connection,
    id: &str,
    role: &str,
    document_kind: Option<&str>,
) -> rusqlite::Result<usize> {
    let file = bae_util::test_ids::test_uuid(id);
    run(conn, &PlantCovenFileParams { id: &file })?;
    run(
        conn,
        &PlantReleaseFileParams {
            id,
            release_id: RELEASE,
            content_type: "text/plain",
            file: &file,
            role,
            document_kind,
            source_audio_layout: None,
            source_audio_content_type: None,
            source_audio_container: None,
            source_audio_duration_ms: None,
            source_audio_sample_rate_hz: None,
            source_audio_bits_per_sample: None,
            source_audio_channels: None,
            created_at: NOW,
        },
    )
}

/// A document states which kind it is, and no other file states one.
#[test]
fn only_a_document_has_a_kind_and_every_document_has_one() {
    let conn = seeded_conn();
    insert_carried_file(&conn, "rip.log", "document", Some("rip_log")).expect("a rip log");
    insert_carried_file(&conn, "notes.nfo", "other", None).expect("a file that is no document");
    for (id, role, kind) in [
        ("kindless.txt", "document", None),
        ("unknown.txt", "document", Some("letter")),
        ("booklet.pdf", "other", Some("text")),
    ] {
        let error = insert_carried_file(&conn, id, role, kind).expect_err(id);
        assert!(error.to_string().contains("CHECK constraint failed"), "{id}: {error}");
    }

    let log = read_file(&conn, "rip.log").unwrap();
    assert_eq!(log.role, crate::ReleaseFileRole::Document(crate::DocumentKind::RipLog));
}

/// A file's name in its release and a release's folder name are valid by the
/// tables' CHECKs, so every read of one finds a name that names a file: no
/// empty name, and no empty name between `/`s. Anything else is a name, the
/// escaping and traversal tokens another device's file system can hold
/// included.
#[test]
fn a_stored_name_that_names_no_file_is_refused() {
    let conn = seeded_conn();
    for name in ["track.flac", "Disc 1/01.flac", "../escape.flac", r"C:\x.flac", "a\0b", "."] {
        insert_carried_file(&conn, name, "other", None).unwrap_or_else(|e| panic!("{name:?}: {e}"));
    }
    for name in ["", "/absolute.flac", "Disc 1/", "Disc 1//01.flac", "/"] {
        let error = insert_carried_file(&conn, name, "other", None).expect_err(name);
        assert!(error.to_string().contains("CHECK constraint failed"), "{name:?}: {error}");
    }
    let set_folder = |name: &str| run(&conn, &SetSourceFolderNameParams { id: RELEASE, name });
    for name in ["Album Title (2020)", "../escape", "a/b"] {
        set_folder(name).unwrap_or_else(|e| panic!("{name:?}: {e}"));
    }
    let error = set_folder("").expect_err("an empty folder name");
    assert!(error.to_string().contains("CHECK constraint failed"), "{error}");
}

/// Plant a size of -5 on each stored file's record: a size no write stores,
/// as coven records each file's length.
fn plant_negative_file_sizes(conn: &Connection) {
    run(conn, &DamageCovenFileSizesParams { size: -5 }).expect("the damage is planted");
}

#[test]
fn a_negative_file_size_is_an_unreadable_column() {
    let conn = seeded_conn();
    insert_file(&conn, "sized", None, None).unwrap();
    plant_negative_file_sizes(&conn);
    let error = read_file(&conn, "sized").unwrap_err();
    assert!(matches!(error, rusqlite::Error::IntegralValueOutOfRange(_, -5)), "{error:?}");
}

/// A track of the seeded release with one audio format and one main segment
/// over a seeded file, all with valid non-negative positions.
fn seeded_audio_segment() -> Connection {
    let conn = seeded_conn();
    insert_file(&conn, "file-1", Some("file"), Some(("audio/flac", 16))).unwrap();
    run(&conn, &PlantMediumParams { id: "medium-1", release_id: RELEASE, created_at: NOW })
        .unwrap();
    let track = PlantTrackParams {
        id: "track-1",
        release_id: RELEASE,
        medium_id: "medium-1",
        created_at: NOW,
    };
    run(&conn, &track).unwrap();
    run(&conn, &PlantAudioFormatParams { id: "format-1", track_id: "track-1", created_at: NOW })
        .unwrap();
    let segment = PlantSegmentParams {
        id: "segment-1",
        audio_format_id: "format-1",
        segment_index: 0,
        role: "main",
        file_id: "file-1",
        start_sample: 0,
        end_sample: Some(44_100),
        start_byte: Some(0),
        end_byte: Some(1_000),
        created_at: NOW,
    };
    run(&conn, &segment).unwrap();
    conn
}

/// Sample and byte positions count from the start of a file, so no row may
/// hold a negative one — whichever device wrote it.
#[test]
fn the_schema_refuses_negative_audio_positions() {
    let conn = seeded_audio_segment();
    for column in ["start_sample", "end_sample", "start_byte", "end_byte"] {
        let update =
            run(&conn, &DamageSegmentPositionParams { id: "segment-1", column, value: -1 });
        assert!(update.is_err(), "a negative {column} was stored");
    }
    for column in ["pregap_samples", "generated_pregap_samples"] {
        let update =
            run(&conn, &DamageAudioFormatValueParams { id: "format-1", column, value: Some(-1) });
        assert!(update.is_err(), "a negative {column} was stored");
    }
}

/// A track's audio has one body and at most one pregap: the schema refuses a
/// second segment of either role for one format, and takes a pregap beside
/// the body, from another file.
#[test]
fn the_schema_refuses_a_second_body_or_pregap_for_one_track() {
    let conn = seeded_audio_segment();
    insert_file(&conn, "file-2", Some("file"), Some(("audio/flac", 16))).unwrap();
    let insert = |id: &str, segment_index: i64, role: &str| {
        let segment = PlantSegmentParams {
            id,
            audio_format_id: "format-1",
            segment_index,
            role,
            file_id: "file-2",
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: NOW,
        };
        run(&conn, &segment)
    };

    let second_body = insert("segment-2", 1, "main").expect_err("a second body was stored");
    assert!(second_body.to_string().contains("UNIQUE constraint failed"), "{second_body}");
    insert("segment-2", 2, "audio_pregap").expect("a pregap beside the body");
    let second_pregap =
        insert("segment-3", 3, "audio_pregap").expect_err("a second pregap was stored");
    assert!(second_pregap.to_string().contains("UNIQUE constraint failed"), "{second_pregap}");
}

/// A track's audio has a sample rate and channels above zero, and a bit depth
/// that is absent (lossy) or 1 to 32, whichever device wrote the row.
#[test]
fn the_schema_refuses_audio_format_values_out_of_range() {
    let conn = seeded_audio_segment();
    let set = |column: &str, value: Option<i64>| {
        run(&conn, &DamageAudioFormatValueParams { id: "format-1", column, value })
    };
    for (column, value) in [
        ("sample_rate", 0),
        ("sample_rate", -44_100),
        ("channels", 0),
        ("channels", -2),
        ("bits_per_sample", 0),
        ("bits_per_sample", -16),
        ("bits_per_sample", 33),
    ] {
        assert!(set(column, Some(value)).is_err(), "{column} = {value} was stored");
    }
    for value in [None, Some(1), Some(16), Some(32)] {
        set("bits_per_sample", value)
            .unwrap_or_else(|error| panic!("bits_per_sample = {value:?}: {error}"));
    }
}

/// A credit's other printed name is its key and its name together: a list
/// holding one without the other is unreadable, as a row holding it is.
#[test]
fn a_credit_name_key_without_its_name_is_unreadable() {
    let credit = |key: &str, name: &str| {
        format!(
            r#"[{{"id":"6c441836-aef7-4239-8a84-5336c4cce52c","name":"Artist Name",
                "sort_name":null,"discogs_artist_id":null,"musicbrainz_artist_id":null,
                "created_at":"2026-01-01T00:00:00Z","credit_key":{key},"credit_name":{name},
                "join":null}}]"#
        )
    };
    let named = crate::records::read::credits_from_json(&credit(r#""alias""#, r#""Alias""#))
        .expect("a credit name with its key reads");
    assert_eq!(
        named[0].credit,
        Some(crate::credits::CreditName { key: "alias".into(), name: "Alias".into() })
    );
    assert_eq!(
        crate::records::read::credits_from_json(&credit("null", "null")).unwrap()[0].credit,
        None
    );
    assert!(crate::records::read::credits_from_json(&credit(r#""alias""#, "null")).is_err());
    assert!(crate::records::read::credits_from_json(&credit("null", r#""Alias""#)).is_err());
}

/// A damaged credit list keeps the decoder's own error as its cause.
#[test]
fn a_damaged_credit_list_is_unreadable_json() {
    let error = crate::records::read::credits_from_json("not json").unwrap_err();
    assert!(matches!(error, bae_store::DbError::UnreadableJson { .. }), "{error:?}");
    let source = std::error::Error::source(&error).expect("the serde error is the source");
    assert!(source.is::<serde_json::Error>());
}
