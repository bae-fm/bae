//! A track always sits on one of its release's medium rows: the schema refuses
//! a write that leaves one on a missing medium, and coven keeps two devices'
//! concurrent writes from leaving one either.
//!
//! A release's media, barcodes and labels are rows, one per place in their
//! list: a save replaces the whole list, and reaches every other device.

use crate::records::test_statements::*;
use crate::test_fixtures::*;
use bae_store::test_fixtures::*;
use bae_store::WriteStatements as _;
use bae_types::pressing::{DiscogsDetail, MediaCount, Medium};

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
            time::UtcDateTime::now(),
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
    track_number: u32,
    medium: &str,
) -> Result<(), bae_store::DbError> {
    let track = crate::DbTrack {
        medium_id: medium.to_string(),
        ..crate::DbTrack::new_test(RELEASE, track, "Track Title", track_number)
    };
    crate::records::write::insert_track_row(conn, &track)
}

/// Move every track on the medium `from` to `to`, then remove `from`, in
/// `conn`'s write.
fn empty_and_remove_medium(
    conn: &coven::CovenWrite<'_>,
    from: &str,
    to: &str,
) -> Result<(), bae_store::DbError> {
    conn.run(&MoveTracksBetweenMediumsParams { from, to })?;
    crate::records::write::delete_release_medium_row(conn, from)
}

/// Removing a medium a track still sits on fails at commit; moving the track
/// first, in the same write, removes it.
#[tokio::test]
async fn a_medium_still_holding_a_track_cannot_be_removed() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let [disc_one, disc_two] = seed_two_discs(&db).await;

    let second = disc_two.clone();
    let refused =
        db.call(move |conn| crate::records::write::delete_release_medium_row(conn, &second)).await;
    assert!(refused.is_err(), "the track on disc 2 keeps its medium");

    db.call(move |conn| empty_and_remove_medium(conn, &disc_two, &disc_one)).await.unwrap();
    assert_eq!(
        records.get_release_mediums(RELEASE).await.unwrap().len(),
        1,
        "the emptied disc is gone"
    );
}

/// The tracks of either device that sit on no medium row its library holds.
async fn tracks_on_missing_mediums(db: &bae_store::Store) -> Vec<String> {
    let rows = db.read_rows(TracksOnMissingMediumsParams {}).await.unwrap();
    rows.into_iter().map(|row| row.id).collect()
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
    devices.converge_rows(|| TracksAndMediumsParams {}).await;

    devices.pause().await;
    let (kept, removed) = (disc_one, disc_two.clone());
    devices.a().call(move |conn| empty_and_remove_medium(conn, &removed, &kept)).await.unwrap();
    devices.b().call(move |conn| insert_track(conn, STRAY_TRACK, 2, &disc_two)).await.unwrap();
    devices.resume().await;
    devices.converge_rows(|| TracksAndMediumsParams {}).await;

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

/// The ids of `RELEASE`'s media rows, in the order of their places.
async fn media_ids_of(db: &bae_store::Store) -> Vec<String> {
    let rows = db.read_rows(MediaRowsOfReleaseParams { release: RELEASE.to_string() });
    rows.await.unwrap().into_iter().map(|row| row.id).collect()
}

/// The ids of `RELEASE`'s Discogs detail rows, in the order of their places.
async fn discogs_detail_ids_of(db: &bae_store::Store) -> Vec<String> {
    let rows = db.read_rows(DiscogsDetailRowsOfReleaseParams { release: RELEASE.to_string() });
    rows.await.unwrap().into_iter().map(|row| row.id).collect()
}

/// The ids of `RELEASE`'s barcode rows, in the order of their places.
async fn barcode_ids_of(db: &bae_store::Store) -> Vec<String> {
    let rows = db.read_rows(BarcodeRowsOfReleaseParams { release: RELEASE.to_string() });
    rows.await.unwrap().into_iter().map(|row| row.id).collect()
}

/// `RELEASE`'s media as `db` reads the release.
async fn media_on(db: &crate::Records) -> Vec<MediaCount> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.facts.media
}

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
        assert_eq!(media_ids_of(&db).await, ids, "each place is a row of its own");
        let statement = MediaRowsOfReleaseParams { release: RELEASE.to_string() };
        let rows = db.read_rows(statement).await.unwrap();
        let positions: Vec<i64> = rows.into_iter().map(|row| row.position).collect();
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
    devices.converge_rows(|| MediaRowsParams {}).await;

    set_media(devices.a(), vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)]).await;
    devices.converge_rows(|| MediaRowsParams {}).await;

    assert_eq!(
        media_on(&crate::test_fixtures::test_records(devices.b())).await,
        vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)]
    );
}

/// `RELEASE`'s Discogs details as `db` reads the release.
async fn details_on(db: &crate::Records) -> Vec<DiscogsDetail> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.facts.discogs_details
}

/// A save of a release's Discogs details makes them exactly the list it
/// gives, in its order, one row a detail.
#[tokio::test]
async fn a_discogs_detail_save_replaces_the_list() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = crate::test_fixtures::test_records(&db);

    for details in [
        vec![DiscogsDetail::Reissue],
        vec![DiscogsDetail::Stereo, DiscogsDetail::Reissue, DiscogsDetail::Mono],
        vec![DiscogsDetail::Mono, DiscogsDetail::Stereo],
        vec![],
    ] {
        let ids = row_ids(details.len());
        let (given, written) = (ids.clone(), details.clone());
        db.call(move |conn| {
            crate::records::write::replace_release_discogs_details(conn, RELEASE, &written, &given)
        })
        .await
        .unwrap();
        assert_eq!(details_on(&records).await, details);
        assert_eq!(discogs_detail_ids_of(&db).await, ids, "each place is a row of its own");
    }
}

/// A change of one choice of what a release's pressing is lands on what the
/// library holds, and rewrites only the list it changes.
#[tokio::test]
async fn a_pressing_fact_change_rewrites_only_what_it_changes() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let records = crate::test_fixtures::test_records(&db);
    let media_ids = set_media(&db, vec![count(Medium::Cd, 2)]).await;

    let held = records
        .change_release_pressing_facts(RELEASE, |facts| {
            facts.discogs_details.push(DiscogsDetail::Stereo)
        })
        .await
        .unwrap();

    assert!(held);
    assert_eq!(media_on(&records).await, vec![count(Medium::Cd, 2)]);
    assert_eq!(media_ids_of(&db).await, media_ids, "the media rows stay");
    assert_eq!(details_on(&records).await, vec![DiscogsDetail::Stereo]);
    let detail_ids = discogs_detail_ids_of(&db).await;

    records
        .change_release_pressing_facts(RELEASE, |facts| facts.media.push(count(Medium::Dvd, 1)))
        .await
        .unwrap();

    assert_eq!(media_on(&records).await, vec![count(Medium::Cd, 2), count(Medium::Dvd, 1)]);
    assert_eq!(discogs_detail_ids_of(&db).await, detail_ids);
}

#[tokio::test]
async fn a_pressing_fact_change_of_a_release_the_library_lacks_says_so() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);

    let held = records.change_release_pressing_facts(RELEASE, |_| ()).await.unwrap();

    assert!(!held);
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
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.typed.barcodes
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
        assert_eq!(barcode_ids_of(&db).await, ids, "each place is a row of its own");
    }
}

/// Make `RELEASE`'s labels `labels` on `db`, as an edit writes them.
async fn set_labels(db: &bae_store::Store, labels: Vec<bae_types::pressing::ReleaseLabel>) {
    let pressing = bae_types::pressing::Pressing {
        typed: bae_types::pressing::TypedPressing { labels: labels.clone(), ..Default::default() },
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
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.typed.labels
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

/// The release editor reads the file a track's body plays from, and where in
/// that file the body starts: for a track whose pregap sits at the end of the
/// previous file, the editor reads its body's file, not its pregap's.
#[cfg(desktop)]
#[tokio::test]
async fn the_editor_reads_which_file_holds_a_track_s_body_and_where_it_starts() {
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
            created_at: time::UtcDateTime::now(),
        };
        aggregate_ordering::insert_file(&db, &tmp, file.clone(), 1).await;
        files.push(file);
    }
    let format = crate::DbAudioFormat::new(
        TRACK_A,
        bae_util::content_type::ContentType::Flac,
        std::num::NonZeroU32::new(44_100).unwrap(),
        std::num::NonZeroU32::new(16),
        std::num::NonZeroU32::new(2).unwrap(),
        bae_util::test_ids::test_uuid("format"),
        time::UtcDateTime::now(),
    );
    let segment = |index: i64,
                   role: DbAudioSegmentRole,
                   file: &NewReleaseFile,
                   start_sample: u64| crate::DbAudioSegment {
        id: bae_util::test_ids::test_uuid(&format!("segment-{index}")),
        audio_format_id: format.id.clone(),
        segment_index: index,
        role,
        file_id: file.id.clone(),
        start_sample,
        end_sample: None,
        start_byte: None,
        end_byte: None,
        created_at: time::UtcDateTime::now(),
    };
    records
        .insert_audio_format_with_segments_for_test(
            &format,
            &[
                segment(0, DbAudioSegmentRole::AudioPregap, &files[0], 0),
                // 10:22 into the file, at 44.1 kHz.
                segment(1, DbAudioSegmentRole::Main, &files[1], 622 * 44_100),
            ],
        )
        .await
        .unwrap();

    let tracks = db
        .read(|sql| Ok(crate::records::editor_release_on(&sql, RELEASE)?.expect("the release")))
        .await
        .unwrap()
        .decoded()
        .unwrap()
        .tracks;

    let track = tracks.iter().find(|track| track.track.id == TRACK_A).expect("track A");
    assert_eq!(track.main_segment_count, 1);
    let body = track.main_segment.as_ref().expect("the track's body");
    assert_eq!(body.file.as_ref().map(|file| file.name.as_str()), Some("02 Track Title B.flac"));
    assert_eq!(body.start_ms, 622_000);
}
