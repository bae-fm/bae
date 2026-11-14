use super::*;
use crate::test_fixtures::aggregate_ordering::*;
use crate::test_fixtures::live_query::*;
use crate::test_fixtures::*;
use bae_store::test_fixtures::live_query::next_run_is_only_the_request;
use bae_store::test_fixtures::*;
use rusqlite::params;

#[tokio::test]
async fn release_file_source_audio_facts_round_trip_without_track_rows() {
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let source_audio = crate::source_audio::SourceAudioFile {
        layout: Some(crate::source_audio::SourceAudioLayout::Cue),
        content_type: bae_util::content_type::ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
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
            role: crate::ReleaseFileRole::Audio,
            source_audio: Some(source_audio.clone()),
            created_at: chrono::Utc::now(),
        },
        12_345,
    )
    .await;

    let files = records.get_files_for_release(RELEASE_A).await.unwrap();
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
        db.read(|sql| crate::records::read::get_release_records_on(&sql, RELEASE_A)).await.unwrap();
    assert_eq!(
        records.iter().map(|record| record.catalog()).collect::<Vec<_>>(),
        [
            bae_catalogs::catalog::Catalog::MusicBrainz,
            bae_catalogs::catalog::Catalog::Discogs,
            bae_catalogs::catalog::Catalog::Spotify,
        ]
    );

    // One list for each id asked, in the order asked, an empty one for a
    // release no catalog describes.
    let lists = db
        .read(|sql| {
            crate::records::read::get_releases_records_on(
                &sql,
                &[RELEASE_A.to_string(), "no-such-release".to_string(), RELEASE_A.to_string()],
            )
        })
        .await
        .unwrap();
    assert_eq!(lists.len(), 3);
    assert_eq!(lists[0].len(), 3);
    assert!(lists[1].is_empty());
    assert_eq!(lists[0], lists[2]);

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::records::read::RELEASES_RECORDS_IN_ORDER),
                [serde_json::json!([RELEASE_A]).to_string()],
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
    let records = crate::test_fixtures::test_records(&db);
    assert_eq!(
        records.track_neighbors(TRACK_A).await.unwrap(),
        Some(crate::records::track::TrackNeighbors {
            is_first: true,
            next: Some(TRACK_B.to_string()),
        })
    );
    assert_eq!(
        records.track_neighbors(TRACK_B).await.unwrap(),
        Some(crate::records::track::TrackNeighbors { is_first: false, next: None })
    );
    assert_eq!(records.count_tracks_for_release(RELEASE_A).await.unwrap(), 2);

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::records::track::TRACK_NEIGHBORS),
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
    let records = crate::test_fixtures::test_records(&db);
    insert_files_out_of_natural_order(&db, tmp.path()).await;
    let names = |files: &[DbFile]| -> Vec<String> {
        files.iter().map(|f| f.original_filename.clone()).collect()
    };

    let read = db
        .read(|sql| crate::records::read::get_files_for_release_on(&sql, RELEASE_A))
        .await
        .unwrap();
    assert_eq!(names(&read), FILES_IN_NATURAL_ORDER, "the read");
    let listed = records.get_files_for_release(RELEASE_A).await.unwrap();
    assert_eq!(names(&listed), FILES_IN_NATURAL_ORDER, "the release's file list");

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::records::read::RELEASE_FILES_IN_ORDER),
                [serde_json::json!([RELEASE_A]).to_string()],
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
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);

    let track = DbTrack::new_test("missing-release", TRACK_A, "Track Title A", 1);
    let error = records
        .insert_track(&track)
        .await
        .expect_err("track insert without a release must violate the foreign key");

    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "expected a foreign-key violation, got {error}"
    );
}

/// A track's medium is one of its own release's: a medium row of another
/// release is refused.
#[tokio::test]
async fn a_track_cannot_sit_on_another_releases_medium() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_releases(&records, ALBUM_A, "Album", &[RELEASE_A, RELEASE_B])
        .await;
    for release in [RELEASE_A, RELEASE_B] {
        records.insert_release_medium(&crate::DbReleaseMedium::new_test(release)).await.unwrap();
    }

    let mut track = DbTrack::new_test(RELEASE_A, TRACK_A, "Track Title A", 1);
    track.medium_id = crate::first_medium_id(RELEASE_B);
    let error = records
        .insert_track(&track)
        .await
        .expect_err("a track on another release's medium must violate the foreign key");

    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "expected a foreign-key violation, got {error}"
    );
}

/// A medium id names one medium: another release cannot hold a row with it.
#[tokio::test]
async fn a_medium_id_belongs_to_one_release() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_releases(&records, ALBUM_A, "Album", &[RELEASE_A, RELEASE_B])
        .await;
    records.insert_release_medium(&crate::DbReleaseMedium::new_test(RELEASE_A)).await.unwrap();

    let mut medium = crate::DbReleaseMedium::new_test(RELEASE_B);
    medium.id = crate::first_medium_id(RELEASE_A);
    let error = records
        .insert_release_medium(&medium)
        .await
        .expect_err("a second release cannot hold the same medium id");
    assert!(error.to_string().contains("UNIQUE constraint failed"), "{error}");
}

/// Every connection to a library knows the natural order bae's schema names:
/// numbers by value, letters without regard to case, then bytes.
#[tokio::test]
async fn coven_connection_orders_by_bae_natural_order() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
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
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        db.subscribe_reconfigurable(0_u8, |_, sql| {
            Ok(sql.query_row(
                "SELECT title FROM albums WHERE id = ?1",
                params![ALBUM_ID],
                |row| row.get::<_, String>(0),
            )?)
        });
    assert_eq!(live.next().await.expect("the query stays open").result.unwrap(), "Album Title");

    exec(&db, "UPDATE albums SET title = 'Renamed Other Album' WHERE id = ?1", &[OTHER_ALBUM_ID])
        .await;

    let moved = next_run_is_only_the_request(&mut live, 1).await;
    assert_eq!(moved.result.unwrap(), "Album Title");
}

/// A work's credit keeps its role: every role bae names is one the table
/// accepts, a role in the catalog's words and the qualifiers read back as they
/// were written.
#[tokio::test]
async fn a_work_credit_reads_back_with_its_role_and_qualifiers() {
    use bae_types::credit_role::{CreditRole, CreditRoleKind, CreditedAs, RoleQualifier};
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let stated = CreditedAs {
        role: CreditRole::Stated("adapter".to_string()),
        qualifiers: vec![RoleQualifier::Stated("lead vocals".to_string())],
    };
    let mut library = crate::test_fixtures::library::RecordsFixture::new()
        .work(WORK_A, "Work Title")
        .artist(ARTIST_COMPOSER, "Composer Name");
    for kind in CreditRoleKind::ALL {
        library = library.work_credit(WORK_A, ARTIST_COMPOSER, CreditedAs::of(*kind));
    }
    library.work_credit(WORK_A, ARTIST_COMPOSER, stated.clone()).write(&records).await;

    let stored = records
        .store
        .read(|sql| {
            sql.query(
                "SELECT role, role_words, qualifiers FROM work_artists ORDER BY position",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();

    let credits: Vec<CreditedAs> = stored
        .into_iter()
        .map(|(role, words, qualifiers)| {
            CreditedAs::from_columns(&role, words, &qualifiers).unwrap()
        })
        .collect();
    let mut expected: Vec<CreditedAs> =
        CreditRoleKind::ALL.iter().map(|kind| CreditedAs::of(*kind)).collect();
    expected.push(stated);
    assert_eq!(credits, expected);
}

/// The view naming the writing roles holds exactly the roles bae calls writing.
#[tokio::test]
async fn the_writing_roles_view_is_the_vocabularys_writing_roles() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let stored: Vec<String> = db
        .read(|sql| {
            sql.query("SELECT role FROM writing_credit_roles", [], |row| row.get::<_, String>(0))
                .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    let expected: Vec<&str> =
        bae_types::credit_role::CreditRoleKind::writing().map(|kind| kind.key()).collect();
    assert_eq!(stored, expected);
}
