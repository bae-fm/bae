//! A track always sits on one of its release's medium rows: the schema refuses
//! a write that leaves one on a missing medium, and coven keeps two devices'
//! concurrent writes from leaving one either.
//!
//! A release's media, barcodes and labels are rows, one per place in their
//! list: a device's edit of them reaches every other device, and two devices
//! editing different places at once keep both edits.

use crate::test_fixtures::*;
use bae_store::test_fixtures::*;
use bae_types::pressing::{MediaCount, Medium};

const RELEASE: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
const KEPT_TRACK: &str = "4f3c4c52-4d1b-4bd5-9b45-6a6f2f3c1a10";
const MOVED_TRACK: &str = "6b1e1f0e-55a8-4d0f-8a5e-0d2e4a3c7b21";
const STRAY_TRACK: &str = "8d2f6a1b-7c3e-4e5f-9a0b-1c2d3e4f5a6b";

fn medium_id(position: u16) -> String {
    crate::identity::release_medium_id(RELEASE, position)
}

/// The seeded release with two CDs and one track on each.
async fn seed_two_discs(db: &bae_store::Store) {
    seed_artist_album_release(db).await;
    let records = crate::test_fixtures::test_records(db);
    for position in [1u16, 2] {
        let medium = crate::DbReleaseMedium::new(
            RELEASE,
            std::num::NonZeroU16::new(position).unwrap(),
            bae_types::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
            chrono::Utc::now(),
        );
        records.insert_release_medium(&medium).await.unwrap();
    }
    for (track, medium) in [(KEPT_TRACK, 1u16), (MOVED_TRACK, 2)] {
        let track = crate::DbTrack {
            medium_id: medium_id(medium),
            ..crate::DbTrack::new_test(RELEASE, track, "Track Title", 1)
        };
        records.insert_track(&track).await.unwrap();
    }
}

fn insert_track(
    conn: &coven::CovenWrite<'_>,
    track: &str,
    position: i64,
    medium: u16,
) -> Result<(), bae_store::DbError> {
    conn.execute(
        "INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, \
             duration_ms, position, created_at) \
         VALUES (?1, ?2, 'Track Title', ?3, 1, 1, 1000, ?4, '2026-01-01T00:00:00Z')",
        rusqlite::params![track, RELEASE, medium_id(medium), position],
    )?;
    Ok(())
}

/// Removing a medium a track still sits on fails at commit; moving the track
/// first, in the same write, removes it.
#[tokio::test]
async fn a_medium_still_holding_a_track_cannot_be_removed() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    seed_two_discs(&db).await;

    let refused = db
        .call(|conn| {
            conn.execute(
                "DELETE FROM release_mediums WHERE id = ?1",
                rusqlite::params![medium_id(2)],
            )?;
            Ok(())
        })
        .await;
    assert!(refused.is_err(), "the track on disc 2 keeps its medium");

    db.call(|conn| {
        conn.execute(
            "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
            rusqlite::params![medium_id(1), medium_id(2)],
        )?;
        conn.execute("DELETE FROM release_mediums WHERE id = ?1", rusqlite::params![medium_id(2)])?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(
        records.get_release_mediums(RELEASE).await.unwrap().0.len(),
        1,
        "the emptied disc is gone"
    );
}

/// The tracks of either device that sit on no medium row its library holds.
async fn tracks_on_missing_mediums(db: &bae_store::Store) -> Vec<String> {
    db.read(|sql| {
        sql.query(
            "SELECT t.id FROM tracks t \
             LEFT JOIN release_mediums m ON m.id = t.medium_id \
             WHERE m.id IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(bae_store::DbError::from)
    })
    .await
    .unwrap()
}

/// One device removes disc 2, moving its track to disc 1, while the other,
/// apart, writes a new track on disc 2. Once each has the other's writes,
/// neither holds a track on a missing medium.
#[test]
fn concurrent_medium_removal_and_track_write_leave_no_track_on_a_missing_medium() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_concurrent_medium_removal,
    );
}

async fn check_concurrent_medium_removal(devices: &bae_store::test_devices::TwoStores) {
    seed_two_discs(devices.a()).await;
    let rows = "SELECT t.id || ':' || t.medium_id FROM tracks t \
         UNION ALL SELECT 'medium:' || m.id FROM release_mediums m";
    devices.converge(rows).await;

    devices.pause().await;
    devices
        .a()
        .call(|conn| {
            conn.execute(
                "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
                rusqlite::params![medium_id(1), medium_id(2)],
            )?;
            conn.execute(
                "DELETE FROM release_mediums WHERE id = ?1",
                rusqlite::params![medium_id(2)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    devices.b().call(|conn| insert_track(conn, STRAY_TRACK, 2, 2)).await.unwrap();
    devices.resume().await;
    devices.converge(rows).await;

    for device in [devices.a(), devices.b()] {
        assert_eq!(tracks_on_missing_mediums(device).await, Vec::<String>::new());
    }
}

fn count(medium: Medium, count: u32) -> MediaCount {
    MediaCount { medium, count }
}

/// Make `RELEASE`'s media `media` on `db`.
async fn set_media(db: &bae_store::Store, media: Vec<MediaCount>) {
    db.call(move |conn| crate::records::write::replace_release_media(conn, RELEASE, &media))
        .await
        .unwrap();
}

/// `RELEASE`'s media as `db` reads the release.
async fn media_on(db: &crate::Records) -> Vec<MediaCount> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.facts.media
}

/// The rows both devices must agree on.
const MEDIA_ROWS: &str = "SELECT id || ':' || position || ':' || medium || ':' || count \
     FROM release_media ORDER BY id";

/// One device's edit of a release's media reaches the other, which reads the
/// release by them.
#[test]
fn a_media_edit_reaches_the_other_device() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_media_edit_reaches,
    );
}

async fn check_media_edit_reaches(devices: &bae_store::test_devices::TwoStores) {
    seed_artist_album_release(devices.a()).await;
    set_media(devices.a(), vec![count(Medium::Cd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    set_media(devices.a(), vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    assert_eq!(
        media_on(&crate::test_fixtures::test_records(devices.b())).await,
        vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)]
    );
}

/// Two devices, apart, each change a different place in one release's media.
/// Once each has the other's write, both hold both changes.
#[test]
fn concurrent_edits_of_different_places_both_stay() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_concurrent_places,
    );
}

async fn check_concurrent_places(devices: &bae_store::test_devices::TwoStores) {
    seed_artist_album_release(devices.a()).await;
    set_media(devices.a(), vec![count(Medium::Cd, 1), count(Medium::Dvd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    devices.pause().await;
    set_media(devices.a(), vec![count(Medium::Cd, 2), count(Medium::Dvd, 1)]).await;
    set_media(devices.b(), vec![count(Medium::Cd, 1), count(Medium::BluRay, 1)]).await;
    devices.resume().await;
    devices.converge(MEDIA_ROWS).await;

    let both = vec![count(Medium::Cd, 2), count(Medium::BluRay, 1)];
    assert_eq!(media_on(&crate::test_fixtures::test_records(devices.a())).await, both);
    assert_eq!(media_on(&crate::test_fixtures::test_records(devices.b())).await, both);
}

/// Make `RELEASE`'s barcodes `barcodes` on `db`.
async fn set_barcodes(db: &bae_store::Store, barcodes: &[&str]) {
    let barcodes: Vec<String> = barcodes.iter().map(|code| code.to_string()).collect();
    db.call(move |conn| crate::records::write::replace_release_barcodes(conn, RELEASE, &barcodes))
        .await
        .unwrap();
}

/// `RELEASE`'s barcodes as `db` reads the release.
async fn barcodes_on(db: &crate::Records) -> Vec<String> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.barcodes
}

const BARCODE_ROWS: &str = "SELECT id || ':' || position || ':' || barcode \
     FROM release_barcodes ORDER BY id";

/// Two devices, apart, each change a different place in one release's
/// barcodes; once each has the other's write, both hold both changes. A
/// later edit that drops one leaves only the other on both.
#[test]
fn barcode_edits_merge_by_place_and_reach_the_other_device() {
    bae_store::test_devices::run_two_store_test(crate::test_fixtures::TEST_SCHEMA, check_barcodes);
}

async fn check_barcodes(devices: &bae_store::test_devices::TwoStores) {
    seed_artist_album_release(devices.a()).await;
    set_barcodes(devices.a(), &["0123456789012", "5012345678900"]).await;
    devices.converge(BARCODE_ROWS).await;

    devices.pause().await;
    set_barcodes(devices.a(), &["0123456789029", "5012345678900"]).await;
    set_barcodes(devices.b(), &["0123456789012", "5012345678917"]).await;
    devices.resume().await;
    devices.converge(BARCODE_ROWS).await;
    let both = ["0123456789029", "5012345678917"];
    assert_eq!(barcodes_on(&crate::test_fixtures::test_records(devices.a())).await, both);
    assert_eq!(barcodes_on(&crate::test_fixtures::test_records(devices.b())).await, both);

    set_barcodes(devices.b(), &["0123456789029"]).await;
    devices.converge(BARCODE_ROWS).await;
    assert_eq!(
        barcodes_on(&crate::test_fixtures::test_records(devices.a())).await,
        ["0123456789029"]
    );
}

/// Make `RELEASE`'s labels `labels` on `db`, as an edit writes them.
async fn set_labels(db: &bae_store::Store, labels: Vec<bae_types::pressing::ReleaseLabel>) {
    let parts = crate::LabelParts::new(&coven::UuidIds, db.now_utc(), &labels);
    crate::records::artist_resolution::write_resolving_identities(db, move |conn| {
        crate::records::label_resolution::write_release_labels_on(
            &conn,
            RELEASE,
            &labels,
            &crate::records::label_write(&parts),
        )
    })
    .await
    .unwrap();
}

/// `RELEASE`'s labels as `db` reads the release.
async fn labels_on(db: &crate::Records) -> Vec<bae_types::pressing::ReleaseLabel> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.labels
}

const LABEL_ROWS: &str = "SELECT id || ':' || name || ':' || ifnull(discogs_label_id, '') \
     FROM labels UNION ALL \
     SELECT id || ':' || position || ':' || ifnull(label_id, '') || ':' \
         || ifnull(catalog_number, '') FROM release_labels ORDER BY 1";

/// Two devices, apart, each write a label Discogs names onto one release:
/// both make the one label row its id names, and once each has the other's
/// write, both read the release with the label at each place either wrote.
#[test]
fn label_rows_converge_on_both_devices() {
    bae_store::test_devices::run_two_store_test(crate::test_fixtures::TEST_SCHEMA, check_labels);
}

async fn check_labels(devices: &bae_store::test_devices::TwoStores) {
    use bae_types::pressing::ReleaseLabel;
    let rhino = |number: &str| {
        ReleaseLabel::new(Some("Rhino".to_string()), Some(number.to_string()))
            .unwrap()
            .with_ids(None, Some("7190".to_string()))
    };
    seed_artist_album_release(devices.a()).await;
    devices.converge(LABEL_ROWS).await;

    devices.pause().await;
    set_labels(devices.a(), vec![rhino("R2 1")]).await;
    set_labels(
        devices.b(),
        vec![rhino("R2 1"), ReleaseLabel::new(None, Some("XY 3".to_string())).unwrap()],
    )
    .await;
    devices.resume().await;
    devices.converge(LABEL_ROWS).await;

    let both = vec![
        ReleaseLabel::new(Some("Rhino".to_string()), Some("R2 1".to_string())).unwrap(),
        ReleaseLabel::new(None, Some("XY 3".to_string())).unwrap(),
    ];
    assert_eq!(labels_on(&crate::test_fixtures::test_records(devices.a())).await, both);
    assert_eq!(labels_on(&crate::test_fixtures::test_records(devices.b())).await, both);
    let labels: i64 = devices
        .a()
        .read(|sql| Ok(sql.query_row("SELECT count(*) FROM labels", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(labels, 1);
}

/// The release editor reads each segment's role: a track whose pregap is
/// stored at the tail of the file before reads its pregap's file and its
/// body's, each with the role it plays.
#[cfg(desktop)]
#[tokio::test]
async fn the_editor_reads_which_file_holds_a_track_s_body() {
    use crate::{DbAudioSegmentRole, NewReleaseFile};
    const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc";
    let (db, tmp) = aggregate_ordering::release_detail_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let mut files = Vec::new();
    for name in ["01 Track Title A.flac", "02 Track Title B.flac"] {
        let file = NewReleaseFile {
            id: bae_util::test_ids::test_uuid(name),
            release_id: RELEASE.to_string(),
            original_filename: name.to_string(),
            content_type: bae_util::content_type::ContentType::Flac,
            source_audio: Some(crate::source_audio::SourceAudioFile {
                layout: Some(crate::source_audio::SourceAudioLayout::Cue),
                format: bae_types::audio_format::AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
                content_type: bae_util::content_type::ContentType::Flac,
                container: bae_util::audio_container::AudioContainer::Flac,
                duration_ms: 1_000,
            }),
            created_at: chrono::Utc::now(),
        };
        aggregate_ordering::insert_file(&db, tmp.path(), file.clone(), 1).await;
        files.push(file);
    }
    let format = crate::DbAudioFormat::new(
        TRACK_A,
        bae_util::content_type::ContentType::Flac,
        std::num::NonZeroU32::new(44_100).unwrap(),
        std::num::NonZeroU32::new(16),
        std::num::NonZeroU32::new(2).unwrap(),
        bae_util::test_ids::test_uuid("format"),
        chrono::Utc::now(),
    );
    let segment =
        |index: i64, role: DbAudioSegmentRole, file: &NewReleaseFile| crate::DbAudioSegment {
            id: bae_util::test_ids::test_uuid(&format!("segment-{index}")),
            audio_format_id: format.id.clone(),
            segment_index: index,
            role,
            file_id: file.id.clone(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: chrono::Utc::now(),
        };
    records
        .insert_audio_format_with_segments_for_test(
            &format,
            &[
                segment(0, DbAudioSegmentRole::AudioPregap, &files[0]),
                segment(1, DbAudioSegmentRole::Main, &files[1]),
            ],
        )
        .await
        .unwrap();

    let tracks =
        db.read(|sql| crate::records::release_edit_tracks_on(&sql, RELEASE)).await.unwrap();

    let track = tracks.iter().find(|track| track.track.id == TRACK_A).expect("track A");
    let segments: Vec<_> = track
        .segments
        .iter()
        .map(|segment| (segment.role.clone(), segment.file.as_ref().map(|file| file.name.as_str())))
        .collect();
    assert_eq!(
        segments,
        [
            (DbAudioSegmentRole::AudioPregap, Some("01 Track Title A.flac")),
            (DbAudioSegmentRole::Main, Some("02 Track Title B.flac")),
        ]
    );
}
