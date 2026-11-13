//! A track always sits on one of its release's medium rows: the schema refuses
//! a write that leaves one on a missing medium, and coven keeps two devices'
//! concurrent writes from leaving one either.
//!
//! A release's media, barcodes and labels are rows, one per place in their
//! list: a save replaces the whole list, and reaches every other device.

use crate::test_fixtures::*;
use bae_store::test_fixtures::*;
use bae_types::pressing::{MediaCount, Medium};

const RELEASE: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
const KEPT_TRACK: &str = "4f3c4c52-4d1b-4bd5-9b45-6a6f2f3c1a10";
const MOVED_TRACK: &str = "6b1e1f0e-55a8-4d0f-8a5e-0d2e4a3c7b21";
const STRAY_TRACK: &str = "8d2f6a1b-7c3e-4e5f-9a0b-1c2d3e4f5a6b";

/// The ids of the two mediums `seed_two_discs` makes, by position.
async fn seed_two_discs(db: &bae_store::Store) -> [String; 2] {
    seed_artist_album_release(db).await;
    let records = crate::test_fixtures::test_records(db);
    let mut ids = Vec::new();
    for position in [1u16, 2] {
        let medium = crate::DbReleaseMedium::new(
            &coven::UuidIds,
            RELEASE,
            std::num::NonZeroU16::new(position).unwrap(),
            bae_types::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
            chrono::Utc::now(),
        );
        records.insert_release_medium(&medium).await.unwrap();
        ids.push(medium.id);
    }
    for (track, medium) in [(KEPT_TRACK, &ids[0]), (MOVED_TRACK, &ids[1])] {
        let track = crate::DbTrack {
            medium_id: medium.clone(),
            ..crate::DbTrack::new_test(RELEASE, track, "Track Title", 1)
        };
        records.insert_track(&track).await.unwrap();
    }
    [ids[0].clone(), ids[1].clone()]
}

fn insert_track(
    conn: &coven::CovenWrite<'_>,
    track: &str,
    position: i64,
    medium: &str,
) -> Result<(), bae_store::DbError> {
    conn.execute(
        "INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, \
             duration_ms, position, created_at) \
         VALUES (?1, ?2, 'Track Title', ?3, 1, 1, 1000, ?4, '2026-01-01T00:00:00Z')",
        rusqlite::params![track, RELEASE, medium, position],
    )?;
    Ok(())
}

/// Removing a medium a track still sits on fails at commit; moving the track
/// first, in the same write, removes it.
#[tokio::test]
async fn a_medium_still_holding_a_track_cannot_be_removed() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let [disc_one, disc_two] = seed_two_discs(&db).await;

    let second = disc_two.clone();
    let refused = db
        .call(move |conn| {
            conn.execute("DELETE FROM release_mediums WHERE id = ?1", rusqlite::params![second])?;
            Ok(())
        })
        .await;
    assert!(refused.is_err(), "the track on disc 2 keeps its medium");

    db.call(move |conn| {
        conn.execute(
            "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
            rusqlite::params![disc_one, disc_two],
        )?;
        conn.execute("DELETE FROM release_mediums WHERE id = ?1", rusqlite::params![disc_two])?;
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
    let [disc_one, disc_two] = seed_two_discs(devices.a()).await;
    let rows = "SELECT t.id || ':' || t.medium_id FROM tracks t \
         UNION ALL SELECT 'medium:' || m.id FROM release_mediums m";
    devices.converge(rows).await;

    devices.pause().await;
    let (kept, removed) = (disc_one, disc_two.clone());
    devices
        .a()
        .call(move |conn| {
            conn.execute(
                "UPDATE tracks SET medium_id = ?1 WHERE medium_id = ?2",
                rusqlite::params![kept, removed],
            )?;
            conn.execute("DELETE FROM release_mediums WHERE id = ?1", rusqlite::params![removed])?;
            Ok(())
        })
        .await
        .unwrap();
    devices.b().call(move |conn| insert_track(conn, STRAY_TRACK, 2, &disc_two)).await.unwrap();
    devices.resume().await;
    devices.converge(rows).await;

    for device in [devices.a(), devices.b()] {
        assert_eq!(tracks_on_missing_mediums(device).await, Vec::<String>::new());
    }
}

fn count(medium: Medium, count: u32) -> MediaCount {
    MediaCount { medium, count }
}

/// A fresh id for each of `count` rows.
fn row_ids(count: usize) -> Vec<String> {
    use coven::IdSource;
    (0..count).map(|_| coven::UuidIds.new_id().to_string()).collect()
}

/// Make `RELEASE`'s media `media` on `db`.
/// Return the ids it gave the rows.
async fn set_media(db: &bae_store::Store, media: Vec<MediaCount>) -> Vec<String> {
    let ids = row_ids(media.len());
    let given = ids.clone();
    db.call(move |conn| crate::records::write::replace_release_media(conn, RELEASE, &media, &ids))
        .await
        .unwrap();
    given
}

/// The ids of `table`'s rows of `RELEASE`, in the order of their places.
async fn row_ids_of(db: &bae_store::Store, table: &'static str) -> Vec<String> {
    db.read(move |sql| {
        Ok(sql.query(
            &format!("SELECT id FROM {table} WHERE release_id = ? ORDER BY position"),
            [RELEASE],
            |row| row.get(0),
        )?)
    })
    .await
    .unwrap()
}

/// `RELEASE`'s media as `db` reads the release.
async fn media_on(db: &crate::Records) -> Vec<MediaCount> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.facts.media
}

/// The rows both devices must agree on.
const MEDIA_ROWS: &str = "SELECT id || ':' || position || ':' || medium || ':' || count \
     FROM release_media ORDER BY id";

/// A save of a release's media makes them exactly the list it gives, in its
/// order, whether it grows, reorders or empties the list.
#[tokio::test]
async fn a_media_save_replaces_the_list() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = crate::test_fixtures::test_records(&db);

    for media in [
        vec![count(Medium::Cd, 1)],
        vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1), count(Medium::Dvd, 1)],
        vec![count(Medium::Dvd, 1), count(Medium::Vinyl, 2)],
        vec![],
    ] {
        let ids = set_media(&db, media.clone()).await;
        assert_eq!(media_on(&records).await, media);
        assert_eq!(row_ids_of(&db, "release_media").await, ids, "each place is a row of its own");
        let positions: Vec<i64> = db
            .read(|sql| {
                Ok(sql.query(
                    "SELECT position FROM release_media WHERE release_id = ? ORDER BY position",
                    [RELEASE],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(positions, (0..media.len() as i64).collect::<Vec<_>>());
    }
}

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

/// Make `RELEASE`'s barcodes `barcodes` on `db`.
/// Return the ids it gave the rows.
async fn set_barcodes(db: &bae_store::Store, barcodes: &[&str]) -> Vec<String> {
    let barcodes: Vec<String> = barcodes.iter().map(|code| code.to_string()).collect();
    let ids = row_ids(barcodes.len());
    let given = ids.clone();
    db.call(move |conn| {
        crate::records::write::replace_release_barcodes(conn, RELEASE, &barcodes, &ids)
    })
    .await
    .unwrap();
    given
}

/// `RELEASE`'s barcodes as `db` reads the release.
async fn barcodes_on(db: &crate::Records) -> Vec<String> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.barcodes
}

/// A save of a release's barcodes makes them exactly the list it gives, in
/// its order; a save that drops one leaves the others.
#[tokio::test]
async fn a_barcode_save_replaces_the_list() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = crate::test_fixtures::test_records(&db);

    for barcodes in [
        vec!["0123456789012", "5012345678900"],
        vec!["5012345678900", "0123456789012", "0123456789029"],
        vec!["0123456789029"],
        vec![],
    ] {
        let ids = set_barcodes(&db, &barcodes).await;
        assert_eq!(barcodes_on(&records).await, barcodes);
        assert_eq!(
            row_ids_of(&db, "release_barcodes").await,
            ids,
            "each place is a row of its own"
        );
    }
}

/// Make `RELEASE`'s labels `labels` on `db`, as an edit writes them.
async fn set_labels(db: &bae_store::Store, labels: Vec<bae_types::pressing::ReleaseLabel>) {
    let pressing = bae_types::pressing::Pressing {
        labels: labels.clone(),
        ..bae_types::pressing::Pressing::blank()
    };
    let parts = crate::PressingParts::new(&coven::UuidIds, db.now_utc(), &pressing);
    db.write(move |conn| {
        crate::records::label_resolution::write_release_labels_on(
            &conn,
            RELEASE,
            &labels,
            &crate::records::pressing_write(&parts),
        )
    })
    .await
    .unwrap();
}

/// `RELEASE`'s labels as `db` reads the release.
async fn labels_on(db: &crate::Records) -> Vec<bae_types::pressing::ReleaseLabel> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.labels
}

/// A save of a release's labels makes them exactly the list it gives, in its
/// order, a place that states only a catalog number included.
#[tokio::test]
async fn a_label_save_replaces_the_list() {
    use bae_types::pressing::ReleaseLabel;
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = crate::test_fixtures::test_records(&db);
    let named = |name: &str, number: &str| {
        ReleaseLabel::new(Some(name.to_string()), Some(number.to_string())).unwrap()
    };
    let number_only = |number: &str| ReleaseLabel::new(None, Some(number.to_string())).unwrap();

    for labels in [
        vec![named("Label Name", "R2 1")],
        vec![number_only("XY 3"), named("Label Name", "R2 1"), named("Other Label", "OL 9")],
        vec![named("Other Label", "OL 9"), number_only("XY 3")],
        vec![],
    ] {
        set_labels(&db, labels.clone()).await;
        assert_eq!(labels_on(&records).await, labels);
    }
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
            role: crate::ReleaseFileRole::Audio,
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
