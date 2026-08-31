use super::*;

#[tokio::test]
async fn measure_loudness_accepts_complete_audio_with_an_invalid_terminal_packet() {
    crate::audio_codec::init();
    let clean = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-fixtures/audio-format/placeholder-mp3.mp3"
    ))
    .expect("MP3 fixture");
    let frame_start = clean
        .windows(2)
        .position(|bytes| bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
        .expect("MP3 frame");
    let mut damaged = clean.clone();
    damaged.extend_from_slice(&clean[frame_start..frame_start + 16]);
    damaged.extend_from_slice(b"LYRICSBEGININD0000000LYRICS200TAG");

    let temp = tempfile::Builder::new()
        .suffix(".mp3")
        .tempfile()
        .expect("temp file");
    std::fs::write(temp.path(), damaged).expect("damaged MP3 fixture");
    let probe =
        crate::audio_codec::probe_audio_from_path(temp.path().to_str().expect("UTF-8 temp path"))
            .expect("damaged MP3 probe");
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let track = TrackFile::Standalone {
        db_track: crate::db::DbTrack::new_test("release-id", "track-id", "Track Title", Some(1)),
        file_path: temp.path().to_path_buf(),
        source_audio: crate::import::folder_scanner::ScannedAudio {
            content_type: probe.content_type.clone(),
            duration_ms: probe.duration.as_millis() as u64,
            format: crate::album_detail::AudioFormat {
                codec: "MP3".to_string(),
                sample_rate_hz: i64::from(probe.sample_rate),
                bits_per_sample: probe.bits_per_sample.map(i64::from),
                bitrate_kbps: Some(128),
                channels: i64::from(probe.channels),
            },
        },
    };
    let mut formats = vec![crate::db::DbAudioFormat::new(
        "track-id",
        probe.content_type,
        i64::from(probe.sample_rate),
        probe.bits_per_sample.map(i64::from),
        i64::from(probe.channels),
        "format-id".to_string(),
        now,
    )];
    let segments = vec![crate::db::DbAudioSegment {
        id: "segment-id".to_string(),
        audio_format_id: "format-id".to_string(),
        segment_index: 0,
        role: crate::db::DbAudioSegmentRole::Main,
        file_id: "file-id".to_string(),
        start_sample: 0,
        end_sample: None,
        start_byte: None,
        end_byte: None,
        created_at: now,
    }];
    let file_ids = HashMap::from([(temp.path().to_path_buf(), "file-id".to_string())]);
    let source_file_sizes = HashMap::from([(
        temp.path().to_path_buf(),
        std::fs::metadata(temp.path()).unwrap().len(),
    )]);
    let (event_tx, _rx) = broadcast::channel(16);

    let result = measure_loudness(
        &event_tx,
        &mut formats,
        &segments,
        &file_ids,
        &source_file_sizes,
        &[track],
        "candidate",
        "release-id",
        "import-id",
    )
    .await
    .unwrap();

    assert!(result.broken.is_empty());
}
