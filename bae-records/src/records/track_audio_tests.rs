use super::*;
use crate::test_fixtures::{artist_album_release, test_records, RELEASE_A, TRACK_A, TRACK_B};
use sqlive_compose::Statement;

const FLAC: &str = "audio/flac";

/// `RELEASE_A` with `TRACK_A` then `TRACK_B` on its CD, and no audio.
async fn two_tracks() -> (bae_store::Store, bae_util::test_files::TestFolder, Records) {
    let (db, temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release()
        .tracks(RELEASE_A, &[(TRACK_A, "Track Title A"), (TRACK_B, "Track Title B")])
        .write(&records)
        .await;
    (db, temp, records)
}

/// Give `track_id` an audio format whose segments are, in order, a pregap
/// when `pregap`, then its body, each a whole new FLAC file of `RELEASE_A`.
/// Returns the files' ids and the segments' ids, in segment order.
async fn store_audio(
    records: &Records,
    temp: &bae_util::test_files::TestFolder,
    track_id: &str,
    pregap: bool,
) -> (Vec<String>, Vec<String>) {
    let now = time::UtcDateTime::now();
    let roles: &[DbAudioSegmentRole] = if pregap {
        &[DbAudioSegmentRole::AudioPregap, DbAudioSegmentRole::Main]
    } else {
        &[DbAudioSegmentRole::Main]
    };
    let format = DbAudioFormat {
        id: bae_util::test_ids::test_uuid(&format!("format-{track_id}")),
        track_id: track_id.to_string(),
        content_type: ContentType::from_mime(FLAC),
        pregap_samples: pregap.then_some(88_200),
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        bits_per_sample: std::num::NonZeroU32::new(16),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        track_loudness_lufs: Some(-9.5),
        track_peak_linear: Some(0.5),
        created_at: now,
    };
    let mut files = Vec::new();
    let mut segments = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        let name = format!("{track_id}-{index}.flac");
        let original = temp.write(&name, vec![0u8; 100 + index]);
        let mut file = NewReleaseFile::new(
            RELEASE_A,
            &name,
            ContentType::from_mime(FLAC),
            crate::ReleaseFileRole::Audio,
            bae_util::test_ids::test_uuid(&name),
            now,
        );
        file.source_audio = Some(crate::source_audio::SourceAudioFile {
            layout: Some(crate::source_audio::SourceAudioLayout::File),
            content_type: ContentType::from_mime(FLAC),
            container: bae_util::audio_container::AudioContainer::Flac,
            duration_ms: 1_000,
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
        });
        records.insert_file_for_test(&file, &original).await.unwrap();
        segments.push(DbAudioSegment {
            id: bae_util::test_ids::test_uuid(&format!("segment-{name}")),
            audio_format_id: format.id.clone(),
            segment_index: i64::try_from(index).unwrap(),
            role: role.clone(),
            file_id: file.id.clone(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: now,
        });
        files.push(file.id);
    }
    records.insert_audio_format_with_segments_for_test(&format, &segments).await.unwrap();
    (files, segments.into_iter().map(|segment| segment.id).collect())
}

/// A track's audio is its segments in order, each with its file's size and
/// container and its own scrub points, none for a segment that stores none.
#[tokio::test]
async fn a_tracks_audio_reads_its_segments_with_their_files() {
    let (_db, temp, records) = two_tracks().await;
    let (files, segments) = store_audio(&records, &temp, TRACK_A, true).await;
    let points = [
        crate::ScrubPoint { sample: 0, byte: 8, exact_from: 0 },
        crate::ScrubPoint { sample: 40, byte: 90, exact_from: 41 },
    ];
    records
        .insert_scrub_points_for_test(&DbScrubPoints {
            audio_segment_id: segments[1].clone(),
            points: points.to_vec(),
        })
        .await
        .unwrap();

    let audio = records.track_audio(TRACK_A).await.unwrap();

    assert_eq!(audio.release_id, RELEASE_A);
    let read: Vec<_> = audio
        .segments
        .iter()
        .map(|segment| (segment.role.clone(), segment.file_id.clone(), segment.file_size))
        .collect();
    assert_eq!(
        read,
        [
            (DbAudioSegmentRole::AudioPregap, files[0].clone(), 100),
            (DbAudioSegmentRole::Main, files[1].clone(), 101),
        ]
    );
    assert!(audio.segments[0].scrub_points.as_slice().is_empty());
    assert_eq!(audio.segments[1].scrub_points.as_slice(), points);
    assert_eq!(audio.pregap_samples, Some(88_200));
    assert_eq!(audio.track_loudness_lufs, Some(-9.5));
    assert_eq!(audio.album_loudness_lufs, None);
}

#[tokio::test]
async fn a_track_without_audio_names_what_it_lacks() {
    let (_db, _temp, records) = two_tracks().await;

    let missing = records.track_audio("missing-track").await.unwrap_err();
    assert!(matches!(missing, TrackAudioError::TrackNotFound(_)), "{missing}");
    let unheard = records.track_audio(TRACK_A).await.unwrap_err();
    assert!(matches!(unheard, TrackAudioError::NoAudioFormat(_)), "{unheard}");
}

/// A save of one track reads the track, whether its release plays it first,
/// and the audio of the track after it; a save of the release reads every
/// track in playing order.
#[tokio::test]
async fn a_save_reads_a_track_beside_the_next_and_a_release_in_order() {
    let (db, temp, records) = two_tracks().await;
    store_audio(&records, &temp, TRACK_A, false).await;
    store_audio(&records, &temp, TRACK_B, true).await;

    let first = records.track_for_save(TRACK_A).await.unwrap();
    assert!(first.is_first);
    assert_eq!(first.track.audio.track_id, TRACK_A);
    assert_eq!(first.next.map(|next| next.track_id), Some(TRACK_B.to_string()));

    let last = records.track_for_save(TRACK_B).await.unwrap();
    assert!(!last.is_first);
    assert!(last.next.is_none());

    let counter = db.track_work_for_test().await.unwrap();
    records.track_audio(TRACK_B).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    records.track_for_save(TRACK_A).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    let release = records.release_tracks_for_save(RELEASE_A).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    records.track_tags(TRACK_B).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    let ids = [TRACK_B.to_string(), TRACK_A.to_string()];
    records.track_ids_of_albums_and_tracks(&ids).await.unwrap().unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    records.filter_existing_track_ids(&ids).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1);
    drop(counter);
    let tagged: Vec<_> = release
        .iter()
        .map(|track| (track.audio.track_id.as_str(), track.tags.title.as_str(), track.tags.number))
        .collect();
    assert_eq!(tagged, [(TRACK_A, "Track Title A", 1), (TRACK_B, "Track Title B", 2)]);
    assert_eq!(release[1].tags, records.track_tags(TRACK_B).await.unwrap());
    assert_eq!(release[1].tags.total_tracks, 2);
    assert_eq!(release[1].tags.album_title, "Album Title A");
    assert_eq!(release[1].tags.album_year, Some(2026));
    assert_eq!(release[1].tags.disc, None);
}

/// Each read of tracks' audio finds the rows it reads by their keys: none
/// walks a whole table of the library.
#[tokio::test]
async fn the_track_reads_find_their_rows_by_key() {
    let (db, _temp, _records) = two_tracks().await;
    let plans = db
        .read(|sql| {
            let plan = |statement: &str, params: &[(&str, &dyn rusqlite::ToSql)]| {
                sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), params, |row| {
                    row.get::<_, String>(3)
                })
            };
            let track = TRACK_A.to_string();
            let one = OneTrackAudioParams { track: track.clone() };
            let save = TrackAndNextForSaveParams { track };
            let release = ReleaseForSaveParams { release: RELEASE_A.to_string() };
            Ok([
                plan(OneTrackAudio::SQL, &one.params())?,
                plan(TrackAndNextForSave::SQL, &save.params())?,
                plan(ReleaseForSave::SQL, &release.params())?,
            ])
        })
        .await
        .unwrap();
    for plan in plans {
        for table in [
            "tracks",
            "audio_formats",
            "audio_format_segments",
            "audio_segment_scrub_points",
            "release_files",
            "releases",
        ] {
            let walked: Vec<_> =
                plan.iter().filter(|step| step.starts_with(&format!("SCAN {table}"))).collect();
            assert!(walked.is_empty(), "the read walks {table}: {plan:#?}");
        }
    }
}
