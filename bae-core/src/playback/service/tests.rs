use super::*;

fn test_track_info(track_id: &str) -> PlaybackTrackInfo {
    PlaybackTrackInfo {
        track_id: track_id.to_string(),
        track_title: "Track Title".to_string(),
        artist_names: "Artist Name".to_string(),
        artist_id: "artist-id".to_string(),
        album_id: "album-id".to_string(),
        album_title: "Album Title".to_string(),
        cover_image_id: None,
        release_id: "release-id".to_string(),
        side: None,
    }
}

fn test_prepared_track(
    track_id: &str,
    buffer: SharedSparseBuffer,
    buffer_shared: bool,
) -> PlaybackPreparedTrack {
    PlaybackPreparedTrack {
        track_info: test_track_info(track_id),
        segments: vec![PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            buffer,
            buffer_shared,
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
        }],
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sample_rate: 44_100,
        channels: 2,
        pregap_ms: None,
        generated_pregap_ms: None,
        generated_pregap_samples: None,
        duration: std::time::Duration::from_secs(1),
        content_type: crate::util::content_type::ContentType::Flac,
        replay_gain_linear: 1.0,
    }
}

fn finished_decoder_handle() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {})
}

fn test_track_fmt(track_id: &str) -> TrackFmt {
    TrackFmt {
        track_id: track_id.to_string(),
        duration_ms: 1_000,
        pregap_ms: None,
        position_offset: std::time::Duration::ZERO,
        replay_gain_linear: 1.0,
    }
}

#[test]
fn clear_preloaded_next_cancels_held_unshared_buffer() {
    let buffer = create_sparse_buffer(1_024);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let mut preloaded = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone(), false),
        decoder_handle: finished_decoder_handle(),
        source: PreloadedNextSource::Held(source),
    });

    clear_preloaded_next(&mut preloaded, None);

    assert!(preloaded.is_none());
    assert!(buffer.is_cancelled());
}

#[test]
fn clear_preloaded_next_removes_staged_source() {
    let (_current_sink, current_source, _current_ready) = create_track_stream_pair(44_100, 2);
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    let (boundary_tx, _boundary_rx) = tokio_mpsc::unbounded_channel();
    let gapless = Arc::new(Mutex::new(source::PlaybackSource::new(
        current_source,
        test_track_fmt("current-track"),
        boundary_tx,
    )));
    gapless
        .lock()
        .unwrap()
        .stage_next(next_source, test_track_fmt("next-track"));

    let buffer = create_sparse_buffer(1_024);
    let mut preloaded = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone(), false),
        decoder_handle: finished_decoder_handle(),
        source: PreloadedNextSource::Staged,
    });

    clear_preloaded_next(&mut preloaded, Some(&gapless));

    assert!(preloaded.is_none());
    assert!(!gapless.lock().unwrap().has_next());
    assert!(buffer.is_cancelled());
}

#[test]
fn pregap_seek_position_cases() {
    use std::time::Duration;
    // Direct selection skips a positive pregap and otherwise needs no seek.
    let cases = [
        (Some(3000i64), Some(Duration::from_millis(3000))),
        (None, None),
    ];
    for (pregap_ms, expected) in cases {
        assert_eq!(
            pregap_seek_position(pregap_ms),
            expected,
            "pregap_ms={pregap_ms:?}"
        );
    }
}

#[test]
fn track_start_position_cases() {
    use std::time::Duration;

    assert_eq!(
        TrackStart::Direct.position(Some(3000)),
        Duration::from_millis(3000)
    );
    assert_eq!(TrackStart::Direct.position(None), Duration::ZERO);
    assert_eq!(TrackStart::Natural.position(Some(3000)), Duration::ZERO);
    assert_eq!(
        TrackStart::Position(Duration::from_millis(42_000)).position(Some(3000)),
        Duration::from_millis(42_000)
    );
}

#[test]
fn direct_start_skips_audio_and_generated_pregap_segments() {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone(), false);
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            buffer: pregap_buffer,
            buffer_shared: false,
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            buffer: main_buffer.clone(),
            buffer_shared: false,
            start_sample: 44_100,
            end_sample: Some(88_200),
            start_byte: Some(2_000),
            end_byte: Some(4_000),
        },
    ];

    let decode = prepared.decode_params(0, false);

    assert_eq!(decode.leading_silence_frames, 0);
    assert_eq!(decode.segments.len(), 1);
    assert_eq!(decode.segments[0].buffer.id(), main_buffer.id());
    assert_eq!(decode.segments[0].target_sample, 44_100);
    assert_eq!(decode.segments[0].seek_to_byte, Some(2_000));
}

#[test]
fn natural_start_includes_audio_and_generated_pregap_segments() {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone(), false);
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            buffer: pregap_buffer.clone(),
            buffer_shared: false,
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            buffer: main_buffer,
            buffer_shared: false,
            start_sample: 44_100,
            end_sample: Some(88_200),
            start_byte: Some(2_000),
            end_byte: Some(4_000),
        },
    ];

    let decode = prepared.decode_params(0, true);

    assert_eq!(decode.leading_silence_frames, 441);
    assert_eq!(decode.segments.len(), 2);
    assert_eq!(decode.segments[0].buffer.id(), pregap_buffer.id());
    assert_eq!(decode.segments[0].target_sample, 1_000);
    assert_eq!(decode.segments[0].seek_to_byte, Some(100));
}

// Seek tests for SparseStreamingBuffer integration
use crate::playback::sparse_buffer::SparseStreamingBuffer;

#[test]
fn test_seek_within_buffer() {
    let buffer = SparseStreamingBuffer::new(10000);
    // Buffer has first 10000 bytes
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek to byte 5000 - should be buffered
    assert!(
        buffer.is_buffered(5000),
        "Position 5000 should be within buffered range"
    );
}

#[test]
fn test_seek_past_buffer() {
    let buffer = SparseStreamingBuffer::new(60000);
    // Buffer has first 10000 bytes
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek to byte 50000 - should NOT be buffered
    assert!(
        !buffer.is_buffered(50000),
        "Position 50000 should be past buffered range"
    );
}

#[test]
fn test_seek_multiple_ranges() {
    let buffer = SparseStreamingBuffer::new(60000);
    // Buffer has 0-10000 and 50000-60000
    buffer.append_at(0, &vec![0u8; 10000]);
    buffer.append_at(50000, &vec![0u8; 10000]);

    // Currently at 55000, seek back to 5000 should reuse first range
    assert!(buffer.is_buffered(5000), "Position 5000 should be buffered");
    assert!(
        buffer.is_buffered(55000),
        "Position 55000 should be buffered"
    );
    assert!(
        !buffer.is_buffered(30000),
        "Position 30000 should NOT be buffered (gap)"
    );
}

#[test]
fn test_seek_back_after_forward_seek() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(90000);

    // Initial download: 0-30000
    buffer.append_at(0, &vec![0u8; 30000]);

    // User seeks forward to byte 70000 - new download starts there
    // Simulating: 70000-90000
    buffer.append_at(70000, &vec![0u8; 20000]);

    // Now we have two ranges: 0-30000 and 70000-90000
    assert_eq!(
        buffer.get_ranges(),
        vec![(0, 30000), (70000, 90000)],
        "Should have two non-contiguous ranges"
    );

    // User seeks back to byte 15000 - should be buffered (first range)
    assert!(buffer.is_buffered(15000), "15000 should be in first range");

    // User seeks to byte 75000 - should be buffered (second range)
    assert!(buffer.is_buffered(75000), "75000 should be in second range");

    // User seeks to byte 50000 - gap between ranges, not buffered
    assert!(!buffer.is_buffered(50000), "50000 should be in the gap");
}

#[test]
fn test_ranges_merge_when_gap_filled() {
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(30000);

    // Initial download: 0-10000
    buffer.append_at(0, &vec![0u8; 10000]);

    // Seek forward creates second range: 20000-30000
    buffer.append_at(20000, &vec![0u8; 10000]);

    assert_eq!(buffer.get_ranges().len(), 2, "Should have two ranges");

    // Original download continues and fills gap: 10000-20000
    buffer.append_at(10000, &vec![0u8; 10000]);

    // Ranges should now be merged
    assert_eq!(buffer.get_ranges().len(), 1, "Ranges should be merged");
    assert_eq!(
        buffer.get_ranges(),
        vec![(0, 30000)],
        "Should be single contiguous range"
    );
}
