use super::*;
use crate::test_fixtures::aggregate_ordering::*;
use crate::test_fixtures::live_query::*;
use crate::test_fixtures::*;
use bae_store::test_fixtures::live_query::next_run_is_only_the_request;
use bae_store::test_fixtures::*;

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
        &tmp,
        NewReleaseFile {
            id: bae_util::test_ids::test_uuid("source-audio-round-trip"),
            release_id: RELEASE_A.to_string(),
            original_filename: "album.flac".to_string(),
            content_type: bae_util::content_type::ContentType::Flac,
            role: crate::ReleaseFileRole::Audio,
            source_audio: Some(source_audio.clone()),
            created_at: time::UtcDateTime::now(),
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
    {
        use bae_catalogs::catalog::Catalog;
        use bae_catalogs::records::{MetadataRef, ReleaseRecord};
        let written = [
            ReleaseRecord::album(&MetadataRef::new(Catalog::Spotify, "album-s")),
            ReleaseRecord::new(&MetadataRef::new(Catalog::Discogs, "pressing-d"), None),
            ReleaseRecord::new(
                &MetadataRef::new(Catalog::MusicBrainz, "pressing-m"),
                Some("group-m".to_string()),
            ),
        ];
        let records = crate::test_fixtures::test_records(&db);
        records.insert_release_records(RELEASE_A, &written).await.unwrap();
    }

    let counter = db.track_work_for_test().await.unwrap();
    let records =
        crate::test_fixtures::test_records(&db).get_release_records(RELEASE_A).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    drop(counter);
    assert_eq!(
        records.iter().map(|record| record.catalog()).collect::<Vec<_>>(),
        [
            bae_catalogs::catalog::Catalog::MusicBrainz,
            bae_catalogs::catalog::Catalog::Discogs,
            bae_catalogs::catalog::Catalog::Spotify,
        ]
    );

    let statement =
        crate::records::read::OneReleaseRecordsParams { release: RELEASE_A.to_string() };
    let plan = bae_store::test_fixtures::query_plan::query_plan(&db, statement).await;
    assert!(
        plan.iter()
            .any(|step| step.starts_with("SEARCH record USING INDEX")
                && step.contains("(release_id=?)")),
        "the read finds the release's records by its index: {plan:?}"
    );
}

/// A release's files come back in natural order of their names, case aside,
/// from the read itself.
#[tokio::test]
async fn release_files_are_read_in_case_insensitive_natural_order() {
    let (db, tmp) = release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    insert_files_out_of_natural_order(&db, &tmp).await;

    let listed = records.get_files_for_release(RELEASE_A).await.unwrap();

    let names: Vec<&str> = listed.iter().map(|file| file.original_filename.as_str()).collect();
    assert_eq!(names, FILES_IN_NATURAL_ORDER);
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
    let rows = db
        .read_rows(crate::records::test_statements::NaturallyOrderedNamesParams {})
        .await
        .unwrap();
    let ordered: Vec<String> = rows.into_iter().map(|row| row.name).collect();
    assert_eq!(ordered, ["Disc 1", "Disc 2", "disc 2", "Disc 10"]);
}

#[tokio::test]
async fn single_table_subscription_ignores_a_different_primary_key() {
    let (db, _temp) = live_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::library::RecordsFixture::new()
        .album(OTHER_ALBUM_ID, "Other Album", None, &[])
        .release(OTHER_ALBUM_ID, OTHER_RELEASE_ID)
        .write(&records)
        .await;
    // The request only gives the test a change of its own to make; the query
    // reads the same row whatever it is.
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> = db
        .subscribe_reconfigurable(0_u8, |_| {
            Ok(crate::records::test_capabilities::TitleOfAlbumParams {
                album: ALBUM_ID.to_string(),
            })
        })
        .process(|_, rows| {
            Ok(bae_store::exactly_one::<crate::records::test_capabilities::TitleOfAlbumParams, _>(
                rows,
            )?
            .title)
        });
    assert_eq!(
        live.next_inline().await.expect("the query stays open").result.unwrap(),
        "Album Title"
    );

    records.rename_album_for_test(OTHER_ALBUM_ID, "Renamed Other Album").await.unwrap();

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
        .read_rows(crate::records::test_statements::WorkCreditRolesParams {})
        .await
        .unwrap();

    let credits: Vec<CreditedAs> = stored
        .into_iter()
        .map(|row| CreditedAs::from_columns(&row.role, row.role_words, &row.qualifiers).unwrap())
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
    let rows = db.read_rows(crate::records::test_statements::WritingRolesParams {}).await.unwrap();
    let stored: Vec<String> = rows.into_iter().map(|row| row.role).collect();
    let expected: Vec<&str> =
        bae_types::credit_role::CreditRoleKind::writing().map(|kind| kind.key()).collect();
    assert_eq!(stored, expected);
}
