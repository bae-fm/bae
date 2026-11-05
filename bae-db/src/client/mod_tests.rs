use super::*;
use crate::test_fixtures::aggregate_ordering::*;
use crate::test_fixtures::live_query::*;
use crate::test_fixtures::*;
use rusqlite::params;

#[tokio::test]
async fn release_file_source_audio_facts_round_trip_without_track_rows() {
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    let source_audio = crate::source_audio::SourceAudioFile {
        layout: Some(crate::source_audio::SourceAudioLayout::Cue),
        content_type: bae_util::content_type::ContentType::Flac,
        duration_ms: 187_654,
        format: bae_types::audio_format::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 96_000,
            bits_per_sample: Some(24),
            bitrate_kbps: None,
            channels: 2,
        },
    };
    insert_file(
        &db,
        tmp.path(),
        NewReleaseFile {
            id: bae_util::test_ids::test_uuid("source-audio-round-trip"),
            release_id: RELEASE_A.to_string(),
            original_filename: "album.flac".to_string(),
            content_type: bae_util::content_type::ContentType::Flac,
            source_audio: Some(source_audio.clone()),
            created_at: chrono::Utc::now(),
        },
        12_345,
    )
    .await;

    let files = db.get_files_for_release(RELEASE_A).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].source_audio, Some(source_audio));
}

/// A release's records come back in the order surfaces list catalogs, read
/// in that order from the index of each release's records, whatever order
/// they were written in.
#[tokio::test]
async fn release_records_are_read_in_catalog_order() {
    let (db, _tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    db.call(|conn| {
        conn.execute_batch(&format!(
            "INSERT INTO release_records (id, release_id, catalog, kind, key, album_key) VALUES
             ('3f0c5d1e-8a2b-4c6d-9e7f-1a2b3c4d5e6f', '{RELEASE_A}', 'spotify', 'album', 'album-s', NULL),
             ('4a1d6e2f-9b3c-4d7e-8f0a-2b3c4d5e6f7a', '{RELEASE_A}', 'discogs', 'pressing', 'pressing-d', NULL),
             ('5b2e7f3a-0c4d-4e8f-9a1b-3c4d5e6f7a8b', '{RELEASE_A}', 'musicbrainz', 'pressing', 'pressing-m', 'group-m');"
        ))?;
        Ok(())
    })
    .await
    .unwrap();

    let records =
        db.read(|sql| crate::client::read::get_release_records_on(&sql, RELEASE_A)).await.unwrap();
    assert_eq!(
        records.iter().map(|record| record.catalog()).collect::<Vec<_>>(),
        [
            bae_catalogs::catalog::Catalog::MusicBrainz,
            bae_catalogs::catalog::Catalog::Discogs,
            bae_catalogs::catalog::Catalog::Spotify,
        ]
    );

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::client::read::RELEASE_RECORDS_IN_ORDER),
                [RELEASE_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING INDEX idx_release_records_release_order")),
        "the read walks the index: {plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the read sorts after reading: {plan:?}"
    );
}

/// A track's place in its release's order is read on the index of each
/// release's track positions: whether it is the first, the track after it,
/// and how many the release has.
#[tokio::test]
async fn a_tracks_neighbors_are_read_from_the_positions_index() {
    let (db, _tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    assert_eq!(
        db.track_neighbors(TRACK_A).await.unwrap(),
        Some(crate::client::track::TrackNeighbors {
            is_first: true,
            next: Some(TRACK_B.to_string()),
        })
    );
    assert_eq!(
        db.track_neighbors(TRACK_B).await.unwrap(),
        Some(crate::client::track::TrackNeighbors { is_first: false, next: None })
    );
    assert_eq!(db.count_tracks_for_release(RELEASE_A).await.unwrap(), 2);

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::client::track::TRACK_NEIGHBORS),
                [TRACK_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter()
            .filter(|step| step.contains("earlier") || step.contains("later"))
            .all(|step| step.contains("tracks_release_position")),
        "the neighbors are read on the positions index: {plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the read sorts after reading: {plan:?}"
    );
}

/// A release's files come back in natural order of their names, case aside,
/// from the read itself, which SQLite orders by walking the index of each
/// release's files in that order: the release's file list and the read under
/// it both show it.
#[tokio::test]
async fn release_files_are_read_in_case_insensitive_natural_order() {
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, tmp.path()).await;
    let names = |files: &[DbFile]| -> Vec<String> {
        files.iter().map(|f| f.original_filename.clone()).collect()
    };

    let read = db
        .read(|sql| crate::client::read::get_files_for_release_on(&sql, RELEASE_A))
        .await
        .unwrap();
    assert_eq!(names(&read), FILES_IN_NATURAL_ORDER, "the read");
    let listed = db.get_files_for_release(RELEASE_A).await.unwrap();
    assert_eq!(names(&listed), FILES_IN_NATURAL_ORDER, "the release's file list");

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::client::read::RELEASE_FILES_IN_ORDER),
                [RELEASE_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING INDEX idx_release_files_release_order")),
        "the read walks the index: {plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the read sorts after reading: {plan:?}"
    );
}

#[tokio::test]
async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
    let (db, _tmp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;

    let track = DbTrack::new_test("missing-release", TRACK_A, "Track Title A", 1);
    let error = db
        .insert_track(&track)
        .await
        .expect_err("track insert without a release must violate the foreign key");

    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "expected a foreign-key violation, got {error}"
    );
}

#[tokio::test]
async fn processing_runs_off_the_runtime_and_preserves_database_errors() {
    let (db, _tmp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let runtime_thread = std::thread::current().id();
    let fetched_thread = db
        .read(|sql| {
            let value = sql.query_row("SELECT 42", [], |row| row.get::<_, i64>(0))?;
            Ok((value, std::thread::current().id()))
        })
        .process(move |(value, reader)| {
            assert_eq!(value, 42);
            assert_ne!(runtime_thread, std::thread::current().id());
            Ok(reader)
        })
        .await
        .unwrap();
    assert_ne!(fetched_thread, std::thread::current().id());

    let error = db
        .read(|sql| Ok(sql.query_row("SELECT 1 WHERE 0", [], |row| row.get::<_, i64>(0))?))
        .process(|_| -> Result<(), DbError> { panic!("a failed fetch must skip processing") })
        .await
        .unwrap_err();
    assert!(matches!(error, DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)));

    let error = db
        .read(|_| Ok(()))
        .process(|()| Err::<(), _>(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)))
        .await
        .unwrap_err();
    assert!(matches!(error, DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)));
}

/// Every connection to a library knows the natural order bae's schema names:
/// numbers by value, letters without regard to case, then bytes.
#[tokio::test]
async fn coven_connection_orders_by_bae_natural_order() {
    let (db, _tmp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let ordered = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT column1 FROM (VALUES ('Disc 10'), ('disc 2'), ('Disc 2'), ('Disc 1'))
                 ORDER BY column1 COLLATE natural_order",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(ordered, ["Disc 1", "Disc 2", "disc 2", "Disc 10"]);
}

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db(crate::test_fixtures::TEST_SCHEMA).await;
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
