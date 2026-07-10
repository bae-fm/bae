use super::*;
use crate::playback::audio_output::{
    audio_event_channel, AudioError, AudioEvent, AudioEventReceiver, AudioEventSender, AudioState,
    AudioStream,
};
// Preview retains the per-track `StreamPipeline`; the test builds one for the
// preview-teardown test via `test_pipeline`.
use crate::playback::stream_pipeline::StreamPipeline;
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
    /// Count of device streams built (`create_stream`) vs source swaps
    /// (`on_source_replaced`), so a test can assert the persistent stream is
    /// rebuilt only on a format change, not on a same-format transition.
    build_count: Arc<std::sync::atomic::AtomicU64>,
    replace_count: Arc<std::sync::atomic::AtomicU64>,
}

impl TestAudioOutput {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(AudioState::Playing),
            volume: std::sync::Mutex::new(1.0),
            build_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            replace_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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
        self.build_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Box::new(TestAudioStream))
    }

    fn on_source_replaced(&mut self) {
        self.replace_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

/// Insert each `(release_id, track_ids)` as album + release + track rows so
/// `get_track_ids` returns each release's tracks in the given order. Tracks are
/// numbered in slice order (the query orders by `side, track_number, id`).
async fn seed_test_releases(database: &crate::db::Database, releases: &[(&str, &[&str])]) {
    use crate::db::{DbAlbum, DbArtist, DbRelease, DbTrack};
    if releases.is_empty() {
        return;
    }
    let artist = DbArtist {
        id: "test-artist-id".to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();
    for (release_id, track_ids) in releases {
        let album = DbAlbum::new_test("Album Title", &artist.id);
        let release = DbRelease::new_test(&album.id, release_id);
        database.insert_album(&album).await.unwrap();
        database.insert_release(&release).await.unwrap();
        for (index, track_id) in track_ids.iter().enumerate() {
            let track =
                DbTrack::new_test(release_id, track_id, "Track Title", Some(index as i32 + 1));
            database.insert_track(&track).await.unwrap();
        }
    }
}

async fn seeded_library_manager(releases: &[(&str, &[&str])]) -> (TempDir, LibraryManager) {
    let home = TempDir::new().unwrap();
    let db_path = home.path().join("playback-service-test.db");
    let database =
        crate::db::Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
    seed_test_releases(&database, releases).await;
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
    seeded_playback_service(&[]).await
}

async fn seeded_playback_service(
    releases: &[(&str, &[&str])],
) -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, library_manager) = seeded_library_manager(releases).await;
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
        output: None,
        slot: PlaybackSlot::Stopped,
        load_generation_counter: 0,
        preloaded_next: None,
        preview,
        main_was_playing_before_preview: false,
        is_muted: false,
        pre_mute_volume: 1.0,
        position_update_interval_ms: 50,
        shared_file_buffers: HashMap::new(),
        last_position_display: Arc::new(std::sync::Mutex::new(None)),
        fetch_arbiter: FetchArbiter::new(),
        starvation_episode: None,
    };
    (home, service, progress_rx)
}

/// Build a `StreamPipeline` over a fresh source for the prepared track's fmt —
/// the per-track unit the preview player still holds. The stub stream/decoder/
/// token come from `new_for_test`.
fn test_pipeline(prepared: &PlaybackPreparedTrack) -> StreamPipeline {
    let (_sink, source, _ready) = create_track_stream_pair(prepared.sample_rate, prepared.channels);
    let track_fmt = prepared.track_fmt(std::time::Duration::ZERO);
    StreamPipeline::new_for_test(Arc::new(Mutex::new(source::PlaybackSource::new(
        source, track_fmt,
    ))))
}

/// An already-exited decoder with a fresh token — the current track's decoder for
/// slot-shaped tests that don't exercise a live decode.
fn test_decoder() -> TrackDecoder {
    TrackDecoder {
        handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

/// Build a persistent `OutputStream` over `source` with a stub device stream and
/// the given audio-events receiver, for tests that drive the drain / seek paths.
fn test_output(
    source: Arc<Mutex<source::PlaybackSource>>,
    audio_events: AudioEventReceiver,
) -> OutputStream {
    OutputStream {
        _stream: Box::new(TestAudioStream),
        source,
        audio_events,
        sample_rate: 44_100,
        channels: 2,
    }
}

/// Build an `Active` slot from a prepared track with a stub decoder. Tests that
/// exercise the persistent output set `service.output` separately.
fn active_slot(prepared: PlaybackPreparedTrack, phase: TrackPhase) -> PlaybackSlot {
    PlaybackSlot::Active(CurrentTrack {
        prepared,
        decoder: test_decoder(),
        phase,
    })
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

fn test_prepared_track(track_id: &str, buffer: SharedSparseBuffer) -> PlaybackPreparedTrack {
    test_prepared_track_with_file(track_id, track_id, buffer)
}

fn test_prepared_track_with_file(
    track_id: &str,
    file_id: &str,
    buffer: SharedSparseBuffer,
) -> PlaybackPreparedTrack {
    PlaybackPreparedTrack {
        track_info: test_track_info(track_id),
        segments: vec![PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: file_id.to_string(),
            buffer,
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
        }],
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
fn discard_preloaded_next_stops_decoder_but_keeps_buffer_alive() {
    let buffer = create_sparse_buffer(1_024);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut preloaded = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: cancel_token.clone(),
        source: PreloadedNextSource::Held(source),
    });

    let prepared = discard_preloaded_next(&mut preloaded, None);

    assert!(preloaded.is_none());
    let prepared = prepared.expect("the prepared track is handed back for buffer release");
    assert_eq!(prepared.track_info.track_id, "next-track");
    assert!(cancel_token.load(std::sync::atomic::Ordering::Acquire));
    // The buffer stays alive: whether it survives is the caller's release
    // decision (release_buffers / stop), not the discard's.
    assert!(!buffer.is_cancelled());
}

#[test]
fn discard_preloaded_next_removes_staged_source() {
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
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    let prepared = discard_preloaded_next(&mut preloaded, Some(&gapless));

    assert!(preloaded.is_none());
    assert!(prepared.is_some());
    assert!(!gapless.lock().unwrap().has_next());
    assert!(!buffer.is_cancelled());
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
    let (mut audio_tx, audio_rx) = audio_event_channel();
    audio_tx.push_required(AudioEvent::TrackCrossing(TrackCrossing {
        finished_fmt: Arc::new(test_track_fmt("finished-track")),
        decode_error_count: 0,
        samples_decoded: 44_100,
        incoming_fmt: Arc::new(test_track_fmt("incoming-track")),
    }));
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        source,
        test_track_fmt("finished-track"),
    )));
    // The crossing event lives in the persistent output's audio-events receiver;
    // the source and receiver survive the track transition.
    service.output = Some(test_output(source, audio_rx));
    service.slot = PlaybackSlot::Active(CurrentTrack {
        prepared: test_prepared_track("finished-track", finished_buffer.clone()),
        decoder: test_decoder(),
        phase: TrackPhase::Playing,
    });
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("incoming-track", incoming_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    service.seek(std::time::Duration::ZERO).await;

    assert_eq!(service.slot.current_track_id().unwrap(), "incoming-track");
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

/// After a track drains naturally, the decoder-completion callback flips the
/// shared audio-state atomic to `Stopped` while the track's bookkeeping is
/// retained (so AutoAdvance / the side-pause decision can still read it). A seek
/// arriving in that window must resume audible playback at the seek target — not
/// rebuild a stream that stays silent because the atomic is still `Stopped`.
#[tokio::test]
async fn seek_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.play_release(
        ContextSource::Release("release-id".to_string()),
        vec!["finished-track".to_string()],
        ContextStart::Index(0),
    );

    let buffer = create_sparse_buffer(1_024);
    // The track drained: phase is Completed with its bookkeeping retained, and
    // the audio callback already flipped the atomic to Stopped.
    service.slot = active_slot(
        test_prepared_track("finished-track", buffer.clone()),
        TrackPhase::Completed,
    );
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Stopped);

    service.seek(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "seeking after a track finished naturally should resume audible playback"
    );
}

/// Skipping to the next track after the current one drained naturally must
/// resume audible playback, not carry the completion's Stopped atomic forward as
/// a silent/paused next track. Drives the real `handle_next` over a Completed
/// slot (the phase a track sits in briefly after natural completion, before
/// AutoAdvance runs) with a preloaded next so no DB lookup is needed. This locks
/// the `TrackPhase::Completed` arm of `current_play_target`; reverting that arm
/// to a paused/stopped target turns the atomic assertion red.
#[tokio::test]
async fn next_after_natural_completion_resumes_audibly() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    service.playback_queue.play_release(
        ContextSource::Release("release-id".to_string()),
        vec!["finished-track".to_string(), "next-track".to_string()],
        ContextStart::Index(0),
    );

    // The current track drained naturally: phase Completed, and the audio
    // callback already flipped the atomic to Stopped.
    let finished_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("finished-track", finished_buffer),
        TrackPhase::Completed,
    );
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Stopped);

    // A next track is preloaded and ready to play without a fresh decode.
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    let next_buffer = create_sparse_buffer(1_024);
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", next_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Held(next_source),
    });

    service.handle_next().await;

    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "Next after natural completion should resume audible playback on the new track"
    );
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == "next-track"
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the next track should be current and Playing"
    );
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
    service.slot = active_slot(
        test_prepared_track_with_file("finished-track", "finished-file", finished_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file("incoming-track", "incoming-file", incoming_buffer),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
    service.slot = active_slot(
        test_prepared_track_with_file("finished-track", "shared-file", shared_buffer.clone()),
        TrackPhase::Playing,
    );
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "incoming-track",
            "shared-file",
            shared_buffer.clone(),
        ),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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

/// A `TrackReady` from a superseded load (same track id, replayed through a
/// fresh load) carries the old generation and must be dropped; only the live
/// load's generation resolves the phase and emits. A same-id replay is exactly
/// the case load identity must reject.
#[tokio::test]
async fn track_ready_with_stale_generation_is_ignored() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let stale = service.next_load_generation();
    let live = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading {
            generation: live,
            target: PlayTarget::Playing,
        },
    );

    service.resolve_track_ready("t".to_string(), stale);
    assert!(
        progress_rx.try_recv().is_err(),
        "a stale-generation TrackReady must not emit"
    );
    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Loading { .. })
        ),
        "the phase must stay Loading after a stale signal"
    );

    service.resolve_track_ready("t".to_string(), live);
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. }
            })
        ),
        "the live load resolves to Playing and emits"
    );
}

/// Pausing during a load collapses the Loading phase to Paused (emitting Paused
/// once); the pending `TrackReady` then no longer matches the phase and is
/// dropped rather than re-emitting a second Paused.
#[tokio::test]
async fn pause_during_load_emits_paused_and_supersedes_track_ready() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let generation = service.next_load_generation();
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );

    service.pause();
    assert!(
        matches!(
            progress_rx.try_recv(),
            Ok(PlaybackProgress::StateChanged {
                state: PlaybackState::Paused {
                    reason: PlaybackPauseReason::Manual,
                    ..
                }
            })
        ),
        "pause during a load emits Paused(Manual)"
    );

    service.resolve_track_ready("t".to_string(), generation);
    assert!(
        progress_rx.try_recv().is_err(),
        "the collapsed load's TrackReady must not emit a second state"
    );
}

fn starved_event(
    track_id: &str,
    starved_ms: u64,
    samples_decoded: u64,
    producer_finished: bool,
) -> AudioEvent {
    AudioEvent::Starved {
        fmt: Arc::new(test_track_fmt(track_id)),
        starved_ms,
        position_ms: 0,
        producer_finished,
        samples_decoded,
        decode_errors: 0,
        has_next: false,
    }
}

fn starvation_ended_event(track_id: &str) -> AudioEvent {
    AudioEvent::StarvationEnded {
        fmt: Arc::new(test_track_fmt(track_id)),
        starved_ms: 0,
        position_ms: 0,
        samples_decoded: 0,
        decode_errors: 0,
    }
}

/// A starvation episode with zero decode progress that persists past the fail
/// threshold is a genuine stall — a decoder wedged for good on a byte buffer
/// that will never produce, not a producer that's merely slow — and must
/// surface a `PlaybackError` and tear playback down rather than log forever
/// with a frozen position bar.
#[tokio::test]
async fn starvation_past_fail_threshold_with_no_progress_escalates_to_error_and_stops() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 30_000, 1_000, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        saw_error,
        "a stalled starvation episode must surface a PlaybackError"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the stalled track must stop"
    );
}

/// `samples_decoded` advancing between `Starved` events proves the producer is
/// alive (e.g. a slow cloud fetch) even though the ring is still starved —
/// this must never escalate, however long the starvation drags on.
#[tokio::test]
async fn starvation_with_advancing_samples_decoded_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 30_000, 1_100, false))
        .await;
    service
        .handle_audio_event(starved_event("t", 60_000, 1_200, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "advancing samples_decoded must never escalate, regardless of starved_ms"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Active(_)),
        "the track must stay active"
    );
}

/// `producer_finished == true` is the completion path — a drained track
/// awaiting `AutoAdvance` — never the stall this watchdog targets.
#[tokio::test]
async fn starvation_with_producer_finished_never_escalates() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service
        .handle_audio_event(starved_event("t", 500, 1_000, true))
        .await;
    service
        .handle_audio_event(starved_event("t", 60_000, 1_000, true))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "producer_finished starvation is the completion path, never escalates"
    );
}

/// A `StarvationEnded` between episodes resets the watchdog clock: the next
/// episode starts fresh rather than inheriting the ended episode's duration.
/// Sabotage — drop the reset on `StarvationEnded` — and the single event below
/// (whose own `starved_ms` already exceeds the threshold) would be read as a
/// continuation of the first episode's stalled baseline and escalate
/// immediately.
#[tokio::test]
async fn starvation_ended_resets_the_episode_clock() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    // First episode starts, then ends (the producer resumed) before ever
    // crossing the fail threshold.
    service
        .handle_audio_event(starved_event("t", 500, 1_000, false))
        .await;
    service
        .handle_audio_event(starvation_ended_event("t"))
        .await;

    // A second, independent episode begins at the same samples_decoded count.
    service
        .handle_audio_event(starved_event("t", 30_000, 1_000, false))
        .await;

    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    assert!(
        !saw_error,
        "StarvationEnded must reset the episode; the first Starved event after \
         it establishes a fresh baseline rather than escalating immediately"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Active(_)),
        "the track must stay active"
    );
}

/// `halt_on_error` is a no-op when the slot is already Stopped, so a failure
/// dispatched after a self-handled stop doesn't emit a duplicate Stopped.
#[tokio::test]
async fn halt_on_error_noops_when_stopped() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    // The slot starts Stopped.
    service.halt_on_error().await;
    assert!(
        progress_rx.try_recv().is_err(),
        "halting an already-stopped slot must emit nothing"
    );
}

/// Natural preview completion (a `PreviewCompleted` command) tears the preview
/// pipeline down and emits `PreviewState::Idle`. This pins the service-side
/// contract the preview listener's Completion arm feeds into: PreviewCompleted →
/// stop() → Idle, with the pipeline gone.
#[tokio::test]
async fn preview_completed_tears_down_and_emits_idle() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline = test_pipeline(&prepared);
    service
        .preview
        .set_active_for_test("preview-file".to_string(), pipeline, buffer.clone());
    assert!(service.preview.is_active());

    service.preview_completed();

    assert!(
        !service.preview.is_active(),
        "completion tears the active preview down"
    );
    assert!(
        buffer.is_cancelled(),
        "completion cancels the preview buffer"
    );
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(
            progress,
            PlaybackProgress::PreviewStateChanged(crate::playback::PreviewState::Idle)
        ) {
            saw_idle = true;
        }
    }
    assert!(saw_idle, "completion emits PreviewState::Idle");
}

/// Table-drive `playback_state()` over each slot/phase.
#[tokio::test]
async fn playback_state_mapping() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);

    service.slot = PlaybackSlot::Stopped;
    assert!(matches!(service.playback_state(), PlaybackState::Stopped));

    service.slot = PlaybackSlot::Loading {
        track_id: "t".to_string(),
        resolved: None,
    };
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading { resolved: None, .. }
    ));

    let generation = service.next_load_generation();
    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Loading {
            generation,
            target: PlayTarget::Playing,
        },
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Loading {
            resolved: Some(_),
            ..
        }
    ));

    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Playing,
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Playing { .. }
    ));

    service.slot = active_slot(
        test_prepared_track("t", buffer.clone()),
        TrackPhase::Paused(PausePhase::Manual),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused {
            reason: PlaybackPauseReason::Manual,
            ..
        }
    ));

    let prompt = PlaybackSidePausePrompt {
        id: "id".to_string(),
        title_key: SIDE_PAUSE_TITLE_KEY,
        side_letter: "B".to_string(),
        message_key: SIDE_PAUSE_VINYL_MESSAGE_KEY,
    };
    service.slot = active_slot(
        test_prepared_track("t", buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: "next".to_string(),
            prompt,
        })),
    );
    assert!(matches!(
        service.playback_state(),
        PlaybackState::Paused {
            reason: PlaybackPauseReason::SideEnded(_),
            ..
        }
    ));
    // Completed is never emitted as a public state, so `playback_state` treats it
    // as unreachable rather than mapping it — no arm to assert here.
}

/// `TrackStart::Direct` deriving its position from `pregap_seek_position` is
/// exercised by its own two cases below (a positive pregap needs a seek to
/// it, no pregap needs none) — the same two cases `pregap_seek_position`
/// itself would need, so there's nothing left for a separate direct test of
/// the free function to add.
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
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer,
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer.clone(),
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
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_samples = Some(441);
    prepared.generated_pregap_ms = Some(10);
    prepared.pregap_ms = Some(1010);
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer.clone(),
            start_sample: 1_000,
            end_sample: None,
            start_byte: Some(100),
            end_byte: None,
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer,
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
    let mut prepared = test_prepared_track("track", buffer);
    prepared.generated_pregap_samples = Some(-1);
    prepared.generated_pregap_ms = Some(10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
}

#[test]
fn generated_pregap_samples_clamps_negative_millisecond_value() {
    let buffer = create_sparse_buffer(1_024);
    let mut prepared = test_prepared_track("track", buffer);
    prepared.generated_pregap_samples = None;
    prepared.generated_pregap_ms = Some(-10);

    assert_eq!(prepared.generated_pregap_samples(), 0);
}

/// A multi-release play concatenates each release's tracks in the input order the
/// releases were chosen, and reports the releases that contributed as the source.
#[tokio::test]
async fn play_releases_concatenates_tracks_in_input_order() {
    let (_home, service, _rx) =
        seeded_playback_service(&[("rel-1", &["t1a", "t1b"]), ("rel-2", &["t2a"])]).await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec!["rel-2".to_string(), "rel-1".to_string()])
        .await;

    assert_eq!(playable, vec!["rel-2", "rel-1"]);
    assert_eq!(tracks, vec!["t2a", "t1a", "t1b"]);
}

/// A release with no tracks (deleted, or never existed) is skipped; the remaining
/// releases still play in order.
#[tokio::test]
async fn play_releases_skips_a_release_without_tracks() {
    let (_home, service, _rx) =
        seeded_playback_service(&[("rel-1", &["t1a"]), ("rel-2", &["t2a", "t2b"])]).await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec![
            "rel-1".to_string(),
            "rel-gone".to_string(),
            "rel-2".to_string(),
        ])
        .await;

    assert_eq!(playable, vec!["rel-1", "rel-2"]);
    assert_eq!(tracks, vec!["t1a", "t2a", "t2b"]);
}

/// The shuffle/restore re-fetch of a multi-release source concatenates each
/// release's current tracks in source order — the same order the initial play
/// built, so a shuffle toggle re-derives over the whole multi-album order.
#[tokio::test]
async fn fetch_source_tracks_concatenates_a_releases_source() {
    let (_home, service, _rx) =
        seeded_playback_service(&[("rel-1", &["t1a", "t1b"]), ("rel-2", &["t2a"])]).await;

    let source = ContextSource::Releases(vec!["rel-1".to_string(), "rel-2".to_string()]);
    let tracks = service.fetch_source_tracks(&source).await.unwrap();

    assert_eq!(tracks, vec!["t1a", "t1b", "t2a"]);
}

/// A `Play` whose context load fails (here: the track doesn't exist) must
/// fail loud — a `PlaybackError` and nothing else — rather than silently
/// falling back to a single-track queue. `get_play_context`'s `Err` covers
/// only DB failures and data-inconsistency (a track missing, or absent from
/// its own release's track list — `release_id` is a required column, so
/// there is no legitimate "track with no context" case), so there is no
/// absence value to preserve a fallback for.
///
/// The discriminator from the old silently-degrading behavior: the old code
/// unconditionally called `emit_queue_update()` after the match (mutating the
/// queue to a bogus single-track entry even on failure); the fix returns
/// before ever touching the queue, so no `QueueUpdated` fires at all.
#[tokio::test]
async fn play_context_load_failure_surfaces_error_without_touching_the_queue() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    service.handle_play("missing-track".to_string()).await;

    let mut saw_error = false;
    let mut saw_queue_update = false;
    let mut saw_playing = false;
    while let Ok(progress) = progress_rx.try_recv() {
        match progress {
            PlaybackProgress::PlaybackError { .. } => saw_error = true,
            PlaybackProgress::QueueUpdated(_) => saw_queue_update = true,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. },
            } => saw_playing = true,
            _ => {}
        }
    }
    assert!(
        saw_error,
        "a failed context load must surface a PlaybackError"
    );
    assert!(
        !saw_queue_update,
        "a failed context load must not mutate the queue with a fallback single-track entry"
    );
    assert!(
        !saw_playing,
        "a failed context load must never reach Playing"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the slot must stay Stopped, not start loading a track whose context failed"
    );
}

/// `attach_track` reuses the one persistent output stream for a same-format
/// transition (swapping the source in place, which fires `on_source_replaced`)
/// and rebuilds the device stream only when the format changes. Drives it
/// directly through a `TestAudioOutput` that counts builds vs replaces.
#[tokio::test]
async fn attach_track_reuses_stream_on_same_format_and_rebuilds_on_change() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, _rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    let replaces = output.replace_count.clone();
    service.audio_output = Box::new(output);

    // First attach: nothing is attached yet, so it builds the stream.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(ts1, test_track_fmt("t1"), 44_100, 2)
        .await
        .expect("the first attach builds a stream");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        1,
        "first attach builds once"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        0,
        "no swap on the first attach"
    );

    // Same format: swap in place, no rebuild.
    let (_s2, ts2, _r2) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(ts2, test_track_fmt("t2"), 44_100, 2)
        .await
        .expect("a same-format attach replaces in place");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        1,
        "a same-format attach reuses the one persistent stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        1,
        "a same-format attach swaps the source (on_source_replaced)"
    );

    // Format change: drop the old stream and build a fresh one.
    let (_s3, ts3, _r3) = create_track_stream_pair(96_000, 2);
    service
        .attach_track(ts3, test_track_fmt("t3"), 96_000, 2)
        .await
        .expect("a format-change attach rebuilds the stream");
    assert_eq!(
        builds.load(Ordering::Relaxed),
        2,
        "a format change rebuilds the device stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        1,
        "a rebuild is not a source swap"
    );
}

/// A default-output-device change rebuilds the persistent stream over the SAME
/// `PlaybackSource` (re-resolving the device), so playback follows the new
/// default without losing position or state — the only path that rebuilds a
/// live stream mid-playback. Builds go up by one, no source swap (the source is
/// reused, not replaced), and the callback's play state is untouched.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn output_device_changed_rebuilds_over_the_same_source() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, _rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    let replaces = output.replace_count.clone();
    service.audio_output = Box::new(output);
    service
        .audio_output
        .set_state(crate::playback::audio_output::AudioState::Playing);

    // Attach a track so there's a live output stream to move.
    let (_s1, ts1, _r1) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(ts1, test_track_fmt("t1"), 44_100, 2)
        .await
        .expect("the first attach builds a stream");
    assert_eq!(builds.load(Ordering::Relaxed), 1, "one build so far");
    let source_before = service.output.as_ref().unwrap().source.clone();

    // The default device changed: rebuild in place.
    service.handle_output_device_changed().await;

    assert_eq!(
        builds.load(Ordering::Relaxed),
        2,
        "a device change rebuilds the device stream"
    );
    assert_eq!(
        replaces.load(Ordering::Relaxed),
        0,
        "a device-change rebuild reuses the source, it is not a swap"
    );
    let source_after = service.output.as_ref().unwrap().source.clone();
    assert!(
        Arc::ptr_eq(&source_before, &source_after),
        "the rebuild reuses the very same PlaybackSource so position/state survive"
    );
    assert_eq!(
        service.audio_output.get_state(),
        crate::playback::audio_output::AudioState::Playing,
        "playback keeps playing across the device switch"
    );
}

/// A device change with nothing playing has no stream to move, so it's a no-op:
/// no build, no source, no error.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn output_device_changed_is_a_noop_when_stopped() {
    use std::sync::atomic::Ordering;

    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let output = TestAudioOutput::new();
    let builds = output.build_count.clone();
    service.audio_output = Box::new(output);

    service.handle_output_device_changed().await;

    assert_eq!(
        builds.load(Ordering::Relaxed),
        0,
        "no stream to rebuild when nothing is playing"
    );
    assert!(service.output.is_none(), "still no output");
    assert!(
        progress_rx.try_recv().is_err(),
        "a no-op device change emits nothing"
    );
}

/// Spawn a stand-in decoder that fills `sink`'s ring until the sink is
/// cancelled, then flags that it exited. A stub output never drains the ring, so
/// the thread parks on a full ring in `push_samples_blocking` — the common
/// steady-state condition, and the only way to prove the source was cancelled
/// (the AVIO cancel token does not unpark a write-blocked decoder; only the
/// sink's cancel flag does).
fn spawn_ring_filling_decoder(
    mut sink: crate::playback::track_stream::TrackSink,
) -> (
    Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let exited = Arc::new(AtomicBool::new(false));
    let exited_in_thread = exited.clone();
    let handle = std::thread::spawn(move || {
        let chunk = vec![0.0f32; 4096];
        while !sink.is_cancelled() {
            sink.push_samples_blocking(&chunk);
        }
        exited_in_thread.store(true, Ordering::Release);
    });
    (exited, handle)
}

async fn await_decoder_exit(exited: &Arc<std::sync::atomic::AtomicBool>) -> bool {
    use std::sync::atomic::Ordering;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !exited.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    exited.load(Ordering::Acquire)
}

/// `stop()` must cancel the output's `PlaybackSource` before dropping it, so the
/// outgoing decoder — which `teardown_current_track` only stopped via its AVIO
/// token — is unparked and exits even when it's blocked writing a full ring.
/// Dropping the source alone abandons the ring but never sets the sink's cancel
/// flag, so a ring-parked decoder would spin forever (leaking its thread and
/// FFmpeg contexts). Sabotage — drop the source-cancel — and this hangs.
#[tokio::test]
async fn stop_cancels_the_output_source_so_a_ring_parked_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let (exited, handle) = spawn_ring_filling_decoder(sink);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        track_stream,
        test_track_fmt("t"),
    )));
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source, audio_rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.stop().await;

    assert!(
        await_decoder_exit(&exited).await,
        "stop() must cancel the output source so a ring-parked decoder exits"
    );
    handle.join().unwrap();
}

/// A format-change rebuild (`attach_track` into a different sample rate/channel
/// count) discards the old output's `PlaybackSource`; it must cancel that source
/// first so its ring-parked decoder exits, for the same reason as `stop()`.
#[tokio::test]
async fn format_change_rebuild_cancels_the_old_source_so_its_decoder_exits() {
    let (_home, mut service, _rx) = test_playback_service().await;

    // An old output at 44.1kHz whose decoder is parked filling a full ring.
    let (sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
    let (exited, handle) = spawn_ring_filling_decoder(sink);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        track_stream,
        test_track_fmt("t"),
    )));
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(source, audio_rx));

    // Attach a track in a DIFFERENT format, forcing a rebuild that drops the old
    // output's source.
    let (_new_sink, new_stream, _new_ready) = create_track_stream_pair(96_000, 2);
    service
        .attach_track(new_stream, test_track_fmt("t2"), 96_000, 2)
        .await
        .expect("the format-change attach rebuilds the stream");

    assert!(
        await_decoder_exit(&exited).await,
        "a format-change rebuild must cancel the discarded source so its decoder exits"
    );
    handle.join().unwrap();
}
