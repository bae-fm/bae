use super::*;
use crate::playback::audio_output::{
    audio_event_channel, AudioError, AudioEvent, AudioEventSender, AudioState,
};
use tempfile::TempDir;

struct TestAudioStream;

impl AudioStream for TestAudioStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(())
    }
}

struct TestAudioOutput {
    state: std::sync::Mutex<AudioState>,
    volume: std::sync::Mutex<f32>,
}

impl TestAudioOutput {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(AudioState::Playing),
            volume: std::sync::Mutex::new(1.0),
        }
    }
}

impl AudioOutput for TestAudioOutput {
    fn create_stream(
        &mut self,
        _source: Arc<Mutex<source::PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        _audio_events: AudioEventSender,
        _position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        Ok(Box::new(TestAudioStream))
    }

    fn set_state(&self, state: AudioState) {
        *self.state.lock().unwrap() = state;
    }

    fn get_state(&self) -> AudioState {
        *self.state.lock().unwrap()
    }

    fn set_volume(&self, volume: f32) {
        *self.volume.lock().unwrap() = volume;
    }

    fn get_volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }
}

async fn test_library_manager() -> (TempDir, LibraryManager) {
    let home = TempDir::new().unwrap();
    let db_path = home.path().join("playback-service-test.db");
    let database =
        crate::db::Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
    let library_id = "playback-service-test".to_string();
    let config = crate::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        coven::LibraryDir::new(home.path().join("library")),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        Arc::new(crate::config::ConfigHandle::new(config)),
        crate::keys::KeyService::new(library_id),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    (home, manager)
}

async fn test_playback_service() -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, library_manager) = test_library_manager().await;
    let queue_ids = library_manager.ids().clone();
    let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
    let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
    let preview = PreviewPlayer::new(progress_tx.clone(), command_tx.clone(), 50);
    let service = PlaybackService {
        library_manager,
        command_tx,
        command_rx,
        progress_tx,
        playback_queue: PlaybackQueue::new(queue_ids),
        current_position_shared: Arc::new(std::sync::Mutex::new(None)),
        audio_output: Box::new(TestAudioOutput::new()),
        stream: None,
        current_prepared: None,
        current_playback_source: None,
        current_decoder_handle: None,
        current_audio_events: None,
        preloaded_next: None,
        preview,
        main_was_playing_before_preview: false,
        is_muted: false,
        pre_mute_volume: 1.0,
        position_update_interval_ms: 50,
        shared_file_buffers: HashMap::new(),
        last_position_display: Arc::new(std::sync::Mutex::new(None)),
        pending_side_pause: None,
        fetch_arbiter: FetchArbiter::new(),
    };
    (home, service, progress_rx)
}

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
    test_prepared_track_with_file(track_id, track_id, buffer, buffer_shared)
}

fn test_prepared_track_with_file(
    track_id: &str,
    file_id: &str,
    buffer: SharedSparseBuffer,
    buffer_shared: bool,
) -> PlaybackPreparedTrack {
    PlaybackPreparedTrack {
        track_info: test_track_info(track_id),
        segments: vec![PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: file_id.to_string(),
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

fn test_resolved_track_audio(
    track_id: &str,
    sample_rate: u32,
    channels: u32,
) -> ResolvedTrackAudio {
    ResolvedTrackAudio {
        track_id: track_id.to_string(),
        release_id: "release-id".to_string(),
        segments: vec![],
        duration_ms: Some(1000),
        pregap_ms: None,
        generated_pregap_ms: None,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate,
        channels,
        content_type: crate::util::content_type::ContentType::Flac,
        track_loudness_lufs: None,
        track_peak_linear: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
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
    let gapless = Arc::new(Mutex::new(source::PlaybackSource::new(
        current_source,
        test_track_fmt("current-track"),
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

#[tokio::test]
async fn seek_drains_pending_gapless_crossing_before_reading_current_track() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    service.playback_queue.play_release(
        ContextSource::Release("release-id".to_string()),
        vec!["finished-track".to_string(), "incoming-track".to_string()],
        ContextStart::Index(0),
    );

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    service.current_prepared = Some(test_prepared_track(
        "finished-track",
        finished_buffer.clone(),
        false,
    ));
    service.current_playback_source = {
        let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
        Some(Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            test_track_fmt("finished-track"),
        ))))
    };
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("incoming-track", incoming_buffer, false),
        decoder_handle: finished_decoder_handle(),
        source: PreloadedNextSource::Staged,
    });

    let (mut audio_tx, audio_rx) = audio_event_channel();
    audio_tx.push_required(AudioEvent::TrackCrossing(TrackCrossing {
        finished_fmt: Arc::new(test_track_fmt("finished-track")),
        decode_error_count: 0,
        samples_decoded: 44_100,
        incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
    }));
    service.current_audio_events = Some(audio_rx);

    service.seek(std::time::Duration::ZERO).await;

    assert_eq!(
        service
            .current_prepared
            .as_ref()
            .unwrap()
            .track_info
            .track_id,
        "incoming-track"
    );
    let mut saw_incoming_seek = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::Seeked { track_id, .. } = progress {
            saw_incoming_seek = track_id == "incoming-track";
        }
    }
    assert!(
        saw_incoming_seek,
        "seek should emit for the crossed-into track"
    );
    assert!(finished_buffer.is_cancelled());
}

#[tokio::test]
async fn gapless_crossing_evicts_finished_track_file_buffer() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.play_release(
        ContextSource::Release("release-id".to_string()),
        vec!["finished-track".to_string(), "incoming-track".to_string()],
        ContextStart::Index(0),
    );

    let finished_buffer = create_sparse_buffer(1_024);
    let incoming_buffer = create_sparse_buffer(1_024);
    service
        .shared_file_buffers
        .insert("finished-file".to_string(), finished_buffer.clone());
    service
        .shared_file_buffers
        .insert("incoming-file".to_string(), incoming_buffer.clone());
    service.current_prepared = Some(test_prepared_track_with_file(
        "finished-track",
        "finished-file",
        finished_buffer.clone(),
        false,
    ));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "incoming-track",
            "incoming-file",
            incoming_buffer,
            false,
        ),
        decoder_handle: finished_decoder_handle(),
        source: PreloadedNextSource::Staged,
    });

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(test_track_fmt("finished-track")),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
        })
        .await;

    assert!(!service.shared_file_buffers.contains_key("finished-file"));
    assert!(service.shared_file_buffers.contains_key("incoming-file"));
    assert!(finished_buffer.is_cancelled());
}

#[tokio::test]
async fn gapless_crossing_keeps_file_buffer_used_by_incoming_track() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.play_release(
        ContextSource::Release("release-id".to_string()),
        vec!["finished-track".to_string(), "incoming-track".to_string()],
        ContextStart::Index(0),
    );

    let shared_buffer = create_sparse_buffer(1_024);
    service
        .shared_file_buffers
        .insert("shared-file".to_string(), shared_buffer.clone());
    service.current_prepared = Some(test_prepared_track_with_file(
        "finished-track",
        "shared-file",
        shared_buffer.clone(),
        false,
    ));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "incoming-track",
            "shared-file",
            shared_buffer.clone(),
            true,
        ),
        decoder_handle: finished_decoder_handle(),
        source: PreloadedNextSource::Staged,
    });

    service
        .handle_track_crossed(TrackCrossing {
            finished_fmt: Arc::new(test_track_fmt("finished-track")),
            decode_error_count: 0,
            samples_decoded: 44_100,
            incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
        })
        .await;

    assert!(service.shared_file_buffers.contains_key("shared-file"));
    assert!(!shared_buffer.is_cancelled());
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
fn resolved_audio_format_rejects_zero_channels() {
    let resolved = test_resolved_track_audio("track-id", 44_100, 0);

    let error = ensure_resolved_audio_format("track-id", &resolved)
        .expect_err("zero channels should be rejected");

    assert!(error
        .to_string()
        .contains("track track-id has unusable audio format"));
}

#[test]
fn resolved_audio_format_rejects_zero_sample_rate() {
    let resolved = test_resolved_track_audio("track-id", 0, 2);

    let error = ensure_resolved_audio_format("track-id", &resolved)
        .expect_err("zero sample rate should be rejected");

    assert!(error
        .to_string()
        .contains("track track-id has unusable audio format"));
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
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer,
            buffer_shared: false,
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
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
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer.clone(),
            buffer_shared: false,
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
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

#[test]
fn generated_pregap_samples_clamps_negative_sample_value() {
    let buffer = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track("track", buffer, false);
    prepared.generated_pregap_samples = Some(-1);
    prepared.generated_pregap_ms = Some(10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
}

#[test]
fn generated_pregap_samples_clamps_negative_millisecond_value() {
    let buffer = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track("track", buffer, false);
    prepared.generated_pregap_samples = None;
    prepared.generated_pregap_ms = Some(-10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
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
