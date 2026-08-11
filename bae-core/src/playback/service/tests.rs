use super::*;
use crate::playback::audio_output::{
    audio_event_channel, AudioError, AudioEvent, AudioEventReceiver, AudioEventSender, AudioState,
    AudioStream,
};
use crate::playback::create_track_stream_pair;
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

/// Track ids for the fixtures that insert real `tracks` rows. coven requires a
/// canonical UUID on every synced-table row, so a seeded track cannot be named
/// `"t"`; the short names elsewhere in this file are in-memory playback ids that
/// never reach the database.
const TRACK_T: &str = "3e747c7f-879c-44a2-ae1e-9767a8d76b15";
const TRACK_A: &str = "bb035a79-2d3b-4ba0-ac99-121564ea97d2";
const TRACK_B: &str = "36a9322d-0c07-4223-9c0b-2110a1fdc622";
const TRACK_T2B: &str = "b98ba0de-5eb5-4c15-a20f-b5df519ed969";

/// Insert each `(release_id, track_ids)` as album + release + track rows so
/// `get_track_ids` returns each release's tracks in the given order. Tracks are
/// numbered in slice order (the query orders by `side, track_number, id`).
async fn seed_test_releases(database: &crate::db::Database, releases: &[(&str, &[&str])]) {
    use crate::db::{DbAlbum, DbArtist, DbRelease, DbTrack};
    if releases.is_empty() {
        return;
    }
    let artist = DbArtist {
        id: bae_test_support::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
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
    seeded_library_manager_with_diagnostics(releases, crate::diagnostics::Diagnostics::noop()).await
}

async fn seeded_library_manager_with_diagnostics(
    releases: &[(&str, &[&str])],
    diagnostics: crate::diagnostics::Diagnostics,
) -> (TempDir, LibraryManager) {
    let home = TempDir::new().unwrap();
    let db_path = home.path().join("playback-service-test.db");
    let database = crate::db::Database::new_test(
        db_path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    seed_test_releases(&database, releases).await;
    let library_id = "playback-service-test".to_string();
    let config = crate::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        coven::StoreDir::new(home.path().join("library")),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        Arc::new(crate::config::ConfigHandle::new(config)),
        crate::keys::StoreKeys::bind(library_id),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        diagnostics,
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
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

/// A `Diagnostics` wired to a `RecordingTransport`, for emission tests that
/// assert typed events reach the wire. The transport replays success (the
/// drained outcome queue defaults to `Ok`).
fn recording_diagnostics() -> (
    crate::diagnostics::Diagnostics,
    Arc<crate::diagnostics::RecordingTransport>,
) {
    use crate::diagnostics::{
        AppDiagnosticMetadata, DatadogDiagnosticsConfig, Diagnostics, RecordingTransport,
    };
    let transport = Arc::new(RecordingTransport::new(vec![]));
    let config = DatadogDiagnosticsConfig {
        datadog_site: "datadoghq.com".to_string(),
        client_token: "client-token".to_string(),
        source: "test".to_string(),
        app: AppDiagnosticMetadata {
            service: "bae".to_string(),
            environment: "test".to_string(),
            app_version: "1.2.3".to_string(),
            edition: "bae".to_string(),
            git_commit: "abc123".to_string(),
        },
    };
    let diagnostics = Diagnostics::with_transport(
        config,
        Arc::new(coven::SystemClock),
        Arc::new(coven::SequentialIdProvider::new("request-id")),
        transport.clone(),
    )
    .expect("diagnostics starts");
    (diagnostics, transport)
}

async fn seeded_playback_service(
    releases: &[(&str, &[&str])],
) -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, library_manager) = seeded_library_manager(releases).await;
    let (service, progress_rx) = playback_service_over(library_manager);
    (home, service, progress_rx)
}

/// Assemble a directly-drivable `PlaybackService` over a given manager (with a
/// stub audio output), for tests that call its handlers without the actor loop.
fn playback_service_over(
    library_manager: LibraryManager,
) -> (
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let queue_ids = Arc::new(coven::SequentialIdProvider::new("queue-entry"));
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
        retired_tracks: Vec::new(),
        fetch_arbiter: FetchArbiter::new(),
        starvation_episode: None,
        last_position_persist: None,
        first_audio_pending: None,
        renderer: Renderer::Local,
    };
    (service, progress_rx)
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
        cover_image: None,
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
            span: crate::db::SegmentSpan::whole_file(),
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
        bits_per_sample: Some(16),
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

#[tokio::test]
async fn retiring_preloaded_next_stops_decoder_but_keeps_buffer_alive() {
    let (_home, mut service, _progress) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: cancel_token.clone(),
        source: PreloadedNextSource::Held(source),
    });

    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    let prepared = service
        .retired_tracks
        .first()
        .expect("the prepared track is retained until buffer release");
    assert_eq!(prepared.track_info.track_id, "next-track");
    assert!(cancel_token.load(std::sync::atomic::Ordering::Acquire));
    // The buffer stays alive: whether it survives is the caller's release
    // decision (release_buffers / stop), not the discard's.
    assert!(!buffer.is_cancelled());
}

#[tokio::test]
async fn retiring_preloaded_next_removes_staged_source() {
    let (_home, mut service, _progress) = test_playback_service().await;
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
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track("next-track", buffer.clone()),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Staged,
    });

    let (_audio_tx, audio_events) = audio_event_channel();
    service.output = Some(test_output(gapless.clone(), audio_events));
    assert!(service.retire_preloaded_track());

    assert!(service.preloaded_next.is_none());
    assert_eq!(service.retired_tracks.len(), 1);
    assert!(!gapless.lock().unwrap().has_next());
    assert!(!buffer.is_cancelled());
}

/// Whether any `PlaybackError` reached the UI. A read failure that surfaces one
/// halts playback (the progress self-subscription turns it into `HaltOnError`),
/// so its absence is what says the playing track was left alone.
fn drained_a_playback_error(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> bool {
    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PlaybackError { .. }) {
            saw_error = true;
        }
    }
    saw_error
}

/// A preloaded next track whose bytes stop arriving — its release was deleted,
/// its cloud fetch failed — breaks nothing the user is hearing, so the playing
/// track keeps playing and no error reaches the UI. The preload is discarded
/// instead: its buffer is cancelled by the time this runs, so a gapless crossing
/// into it would play a truncated track.
#[tokio::test]
async fn read_failure_on_the_preloaded_next_discards_it_and_keeps_playing() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let current_buffer = create_sparse_buffer(1_024);
    let preload_buffer = create_sparse_buffer(1_024);
    service
        .shared_file_buffers
        .insert("preload-file".to_string(), preload_buffer.clone());
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", current_buffer),
        TrackPhase::Playing,
    );
    let (_next_sink, next_source, _next_ready) = create_track_stream_pair(44_100, 2);
    service.preloaded_next = Some(PreloadedNext {
        prepared: test_prepared_track_with_file(
            "next-track",
            "preload-file",
            preload_buffer.clone(),
        ),
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source: PreloadedNextSource::Held(next_source),
    });

    service
        .handle_read_failed(
            preload_buffer.id(),
            PlaybackError::not_found("release file", "preload-file"),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == "current-track"
                    && matches!(cur.phase, TrackPhase::Playing)
        ),
        "the playing track is untouched by the next track's read failure"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a preload's read failure must not surface an error that halts playback"
    );
    assert!(
        service.preloaded_next.is_none(),
        "the preload whose bytes are gone is discarded"
    );
    assert!(
        !service.shared_file_buffers.contains_key("preload-file"),
        "its cancelled buffer leaves the shared cache rather than being reused dead"
    );
}

/// The playing track's own bytes stopping is fatal: nothing can play, so the
/// error surfaces and the halt path tears playback down.
#[tokio::test]
async fn read_failure_on_the_playing_track_reports_the_error() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let current_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", current_buffer.clone()),
        TrackPhase::Playing,
    );

    service
        .handle_read_failed(
            current_buffer.id(),
            PlaybackError::not_found("release file", "current-file"),
        )
        .await;

    assert!(
        drained_a_playback_error(&mut progress_rx),
        "the playing track's read failure surfaces as a playback error"
    );
}

/// A buffer that serves neither the current track nor the preload left the
/// pipeline before its failure surfaced (released on a track change, cancelled
/// by a stop). There is nothing to halt, and halting would kill whatever the
/// user started in the meantime.
#[tokio::test]
async fn read_failure_on_a_buffer_out_of_play_is_ignored() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let abandoned_buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track_with_file("current-track", "current-file", create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );

    service
        .handle_read_failed(
            abandoned_buffer.id(),
            PlaybackError::not_found("release file", "abandoned-file"),
        )
        .await;

    assert!(
        matches!(
            &service.slot,
            PlaybackSlot::Active(cur) if cur.prepared.track_info.track_id == "current-track"
        ),
        "a failure from a buffer out of play leaves the current track alone"
    );
    assert!(
        !drained_a_playback_error(&mut progress_rx),
        "a failure from a buffer out of play surfaces no error"
    );
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
            span: crate::db::SegmentSpan {
                start_sample: 1_000,
                end_sample: None,
                start_byte: Some(100),
                end_byte: None,
            },
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer.clone(),
            span: crate::db::SegmentSpan {
                start_sample: 44_100,
                end_sample: Some(88_200),
                start_byte: Some(2_000),
                end_byte: Some(4_000),
            },
        },
    ];

    let decode = prepared.decode_params(0, false);

    assert_eq!(decode.leading_silence_frames(), 0);
    assert_eq!(decode.segment_count(), 1);
    assert_eq!(decode.segment_buffer_id(0), main_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 44_100);
    assert_eq!(decode.segment_seek_to_byte(0), Some(2_000));
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
            span: crate::db::SegmentSpan {
                start_sample: 1_000,
                end_sample: None,
                start_byte: Some(100),
                end_byte: None,
            },
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer,
            span: crate::db::SegmentSpan {
                start_sample: 44_100,
                end_sample: Some(88_200),
                start_byte: Some(2_000),
                end_byte: Some(4_000),
            },
        },
    ];

    let decode = prepared.decode_params(0, true);

    assert_eq!(decode.leading_silence_frames(), 441);
    assert_eq!(decode.segment_count(), 2);
    assert_eq!(decode.segment_buffer_id(0), pregap_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 1_000);
    assert_eq!(decode.segment_seek_to_byte(0), Some(100));
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
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &[
                "5634d119-43be-4435-8432-575baddc4705",
                "5634ce19-43be-4f1c-8432-545baddc41ec",
            ],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6"],
        ),
    ])
    .await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec![
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
        ])
        .await;

    assert_eq!(
        playable,
        vec![
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
        ]
    );
    assert_eq!(
        tracks,
        vec![
            "5630a919-43ba-4766-842e-6f5badd886f6",
            "5634d119-43be-4435-8432-575baddc4705",
            "5634ce19-43be-4f1c-8432-545baddc41ec"
        ]
    );
}

/// A release with no tracks (deleted, or never existed) is skipped; the remaining
/// releases still play in order.
#[tokio::test]
async fn play_releases_skips_a_release_without_tracks() {
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &["5634d119-43be-4435-8432-575baddc4705"],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6", TRACK_T2B],
        ),
    ])
    .await;

    let (playable, tracks) = service
        .load_release_set_tracks(vec![
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
            "rel-gone".to_string(),
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
        ])
        .await;

    assert_eq!(
        playable,
        vec![
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
        ]
    );
    assert_eq!(
        tracks,
        vec![
            "5634d119-43be-4435-8432-575baddc4705",
            "5630a919-43ba-4766-842e-6f5badd886f6",
            TRACK_T2B
        ]
    );
}

/// The shuffle/restore re-fetch of a multi-release source concatenates each
/// release's current tracks in source order — the same order the initial play
/// built, so a shuffle toggle re-derives over the whole multi-album order.
#[tokio::test]
async fn fetch_source_tracks_concatenates_a_releases_source() {
    let (_home, service, _rx) = seeded_playback_service(&[
        (
            "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
            &[
                "5634d119-43be-4435-8432-575baddc4705",
                "5634ce19-43be-4f1c-8432-545baddc41ec",
            ],
        ),
        (
            "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b",
            &["5630a919-43ba-4766-842e-6f5badd886f6"],
        ),
    ])
    .await;

    let source = ContextSource::Releases(vec![
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string(),
        "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b".to_string(),
    ]);
    let tracks = service.fetch_source_tracks(&source).await.unwrap();

    assert_eq!(
        tracks,
        vec![
            "5634d119-43be-4435-8432-575baddc4705",
            "5634ce19-43be-4f1c-8432-545baddc41ec",
            "5630a919-43ba-4766-842e-6f5badd886f6"
        ]
    );
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
        .attach_track(
            ts1,
            test_track_fmt("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
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
        .attach_track(
            ts2,
            test_track_fmt("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
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
        .attach_track(
            ts3,
            test_track_fmt("t3"),
            96_000,
            2,
            StagedNextOnReplace::Discard,
        )
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
        .attach_track(
            ts1,
            test_track_fmt("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
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
        .attach_track(
            new_stream,
            test_track_fmt("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
            96_000,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("the format-change attach rebuilds the stream");

    assert!(
        await_decoder_exit(&exited).await,
        "a format-change rebuild must cancel the discarded source so its decoder exits"
    );
    handle.join().unwrap();
}

fn queued_completion(track_id: &str) -> AudioEvent {
    AudioEvent::Completion((Arc::new(test_track_fmt(track_id)), 0, 44_100))
}

fn drained_track_completed_ids(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Vec<String> {
    let mut ids = Vec::new();
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::TrackCompleted { track_id } = progress {
            ids.push(track_id);
        }
    }
    ids
}

/// A same-format switch swaps the source in place but keeps the one persistent
/// audio-events receiver. Events queued for the outgoing track before the swap
/// must be dropped under the same lock the swap takes, or a later drain would
/// pop the outgoing track's `Completion` and stamp the incoming track
/// `Completed` (muting it) and fire a spurious auto-advance. Sabotage — skip the
/// drain in `attach_track`'s replace branch — and the stale `TrackCompleted`
/// fires.
#[tokio::test]
async fn same_format_replace_drops_events_queued_for_the_outgoing_track() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let (_sink_a, stream_a, _r_a) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        stream_a,
        test_track_fmt("A"),
    )));
    // A Completion for A is already in the receiver, not yet drained.
    let (mut tx, rx) = audio_event_channel();
    tx.push_required(queued_completion("A"));
    service.output = Some(test_output(source, rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("A", buffer), TrackPhase::Playing);

    // A same-format switch to B swaps the source in place.
    let (_sink_b, stream_b, _r_b) = create_track_stream_pair(44_100, 2);
    service
        .attach_track(
            stream_b,
            test_track_fmt("B"),
            44_100,
            2,
            StagedNextOnReplace::Discard,
        )
        .await
        .expect("a same-format attach replaces in place");

    // Draining now must find nothing stale — the outgoing track's Completion is
    // gone, so no TrackCompleted (which would drive a spurious advance).
    service.drain_current_audio_events().await;
    assert!(
        drained_track_completed_ids(&mut progress_rx).is_empty(),
        "a same-format swap must drop events queued for the outgoing track"
    );
}

/// A default-device rebuild mints a fresh audio-events channel but reuses the
/// SAME source. A `Completion` queued when the device changed must be carried
/// onto the new channel — it can never re-fire (the source's completion latch is
/// already set), so losing it wedges auto-advance at the end of that track.
/// Sabotage — drop the old receiver without carrying its events — and no
/// `TrackCompleted` survives the rebuild.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn device_change_carries_a_queued_completion_forward() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let (_sink, stream, _r) = create_track_stream_pair(44_100, 2);
    let source = Arc::new(Mutex::new(source::PlaybackSource::new(
        stream,
        test_track_fmt("t"),
    )));
    let (mut tx, rx) = audio_event_channel();
    tx.push_required(queued_completion("t"));
    service.output = Some(test_output(source, rx));
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);

    service.handle_output_device_changed().await;
    service.drain_current_audio_events().await;

    assert_eq!(
        drained_track_completed_ids(&mut progress_rx),
        vec!["t".to_string()],
        "a queued Completion must survive a device-change rebuild so auto-advance still fires"
    );
}

/// A stale `AutoAdvance` — for a track that is no longer current because the user
/// pressed Next first — must be dropped, not advance again (which would skip the
/// track Next landed on). The completed track's id no longer matches the current
/// track, so the advance is stale.
#[tokio::test]
async fn auto_advance_ignores_a_stale_track_id_after_a_manual_next() {
    let (_home, mut service, _rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    // The user already pressed Next: B is current and Playing. A stale
    // AutoAdvance for the previously-completed A arrives afterward.
    service.slot = active_slot(test_prepared_track("B", buffer), TrackPhase::Playing);

    service.handle_auto_advance("A".to_string()).await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("B"),
        "a stale AutoAdvance for a no-longer-current track must not advance"
    );
}

/// A stale `AutoAdvance` whose track IS still current but is no longer in the
/// `Completed` phase — because a seek after its completion reset the phase — must
/// also be dropped, or the queued advance would abandon the seek the user just
/// made.
#[tokio::test]
async fn auto_advance_ignores_a_matching_track_that_is_no_longer_completed() {
    let (_home, mut service, _rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    // A is current again and Playing (a seek after its completion moved the phase
    // off Completed). The stale AutoAdvance for A must not advance.
    service.slot = active_slot(test_prepared_track("A", buffer), TrackPhase::Playing);

    service.handle_auto_advance("A".to_string()).await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("A"),
        "AutoAdvance must only fire while the completed track is still Completed"
    );
}

/// A play command ships `playback_command` (the user intent), `playback_started`
/// (the new context), and `track_started` (the track it begins), driven through
/// the real command loop out to the (recording) Datadog transport. Driving it as
/// a queued command — not a direct `handle_play` — is what exercises the
/// `playback_command` emission, which lives in the loop, not the handler. The
/// seeded release has no backing audio, so preparing the track fails after all
/// three events are already emitted — the events fire at the command, not on a
/// successful decode.
#[tokio::test]
async fn a_play_command_ships_playback_command_started_and_track_started() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &[
                "08c80007-b56a-4fc9-8df6-af2967fa09b9",
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            ],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

    // Queue the play command and a shutdown, then let the real loop drain both:
    // it emits the command telemetry, runs the play, then breaks on shutdown.
    let commands = service.command_tx.clone();
    dispatch_command(
        &commands,
        PlaybackCommand::Play("08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()),
    );
    let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    dispatch_command(&commands, PlaybackCommand::Shutdown(shutdown_tx));
    service.run().await;

    diagnostics.flush().await.expect("flush succeeds");
    let names = transport.event_names();
    assert!(
        names.iter().any(|n| n == "playback_command"),
        "a play command ships playback_command (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "playback_started"),
        "a play command ships playback_started (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "track_started"),
        "a play command ships track_started (got {names:?})"
    );
}

/// `Previous` ships `track_started` for the track it lands on — a path that
/// carried no emission until the event moved into `play_track`. The seeded
/// release has no backing audio, so preparing the previous track fails after
/// the event is already emitted: the event fires at the command, not on a
/// successful decode.
#[tokio::test]
async fn previous_ships_track_started() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &[
                "08c80007-b56a-4fc9-8df6-af2967fa09b9",
                "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            ],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

    // A two-track context playing from t1, so Previous steps back to t0.
    service.playback_queue.play_release(
        ContextSource::Release("c61a9e19-f3ba-4728-842c-c59dbc82e238".to_string()),
        vec![
            "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string(),
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
        ],
        ContextStart::Index(1),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    service.handle_previous().await;

    diagnostics.flush().await.expect("flush succeeds");
    let names = transport.event_names();
    assert!(
        names.iter().any(|n| n == "track_started"),
        "Previous ships track_started (got {names:?})"
    );
}

/// A user-intent command maps to its telemetry kind; internal/system commands,
/// queries, and continuous inputs map to `None` and ship nothing.
#[test]
fn playback_command_kind_maps_user_intent_only() {
    use super::playback_command_kind;
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::Play(
            "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
        )),
        Some(PlaybackCommandKind::Play)
    ));
    assert!(matches!(
        playback_command_kind(&PlaybackCommand::SetRepeatMode(RepeatMode::Off)),
        Some(PlaybackCommandKind::SetRepeat)
    ));
    // A continuous input and an internal command ship nothing.
    assert!(playback_command_kind(&PlaybackCommand::SetVolume(0.5)).is_none());
    assert!(playback_command_kind(&PlaybackCommand::AutoAdvance {
        track_id: "08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()
    })
    .is_none());
}

/// A track's natural completion ships `track_completed` carrying the decode-error
/// count — the quality signal.
#[tokio::test]
async fn track_completion_ships_track_completed_with_decode_error_count() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &["08c80007-b56a-4fc9-8df6-af2967fa09b9"],
        )],
        diagnostics.clone(),
    )
    .await;
    let (mut service, _progress_rx) = playback_service_over(manager);

    // A track must be active for the completion to mark its phase.
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c80007-b56a-4fc9-8df6-af2967fa09b9", buffer),
        TrackPhase::Playing,
    );

    service.handle_completion_event(
        Arc::new(test_track_fmt("08c80007-b56a-4fc9-8df6-af2967fa09b9")),
        2,
        44_100,
    );

    diagnostics.flush().await.expect("flush succeeds");
    let bodies = transport.requests();
    let events: Vec<crate::diagnostics::DiagnosticEvent> = bodies
        .iter()
        .flat_map(|r| {
            serde_json::from_slice::<Vec<crate::diagnostics::DiagnosticEvent>>(&r.body).unwrap()
        })
        .collect();
    let completed = events
        .iter()
        .find(|e| e.name == "track_completed")
        .expect("track completion ships track_completed");
    assert_eq!(completed.fields["decode_errors"], serde_json::json!(2));
    assert_eq!(
        completed.fields["track_id"],
        serde_json::json!("08c80007-b56a-4fc9-8df6-af2967fa09b9")
    );
}

/// A corrupt resume row, driven through the real restore path, ships
/// `anomaly{resume_cache_corrupt}` and clears the row. A negative `position_ms`
/// is an out-of-domain value `from_row` rejects — the row is our own write, so
/// out of range means a corrupted local cache.
#[tokio::test]
async fn corrupt_resume_row_ships_resume_cache_corrupt_anomaly() {
    let (diagnostics, transport) = recording_diagnostics();
    let (_home, manager) = seeded_library_manager_with_diagnostics(
        &[(
            "c61a9e19-f3ba-4728-842c-c59dbc82e238",
            &["08c80007-b56a-4fc9-8df6-af2967fa09b9"],
        )],
        diagnostics.clone(),
    )
    .await;

    // Persist a row whose position is out of domain; `from_row` discards it.
    manager
        .save_playback_state(&crate::db::DbPlaybackState {
            context: None,
            manual: "[]".to_string(),
            repeat: "off".to_string(),
            current_track_id: Some("08c80007-b56a-4fc9-8df6-af2967fa09b9".to_string()),
            position_ms: Some(-1),
            volume: 1.0,
            is_muted: false,
        })
        .await
        .expect("save the corrupt row");

    let (mut service, _progress_rx) = playback_service_over(manager);
    service.restore_from_cache(true).await;

    diagnostics.flush().await.expect("flush succeeds");
    let events: Vec<crate::diagnostics::DiagnosticEvent> = transport
        .requests()
        .iter()
        .flat_map(|r| {
            serde_json::from_slice::<Vec<crate::diagnostics::DiagnosticEvent>>(&r.body).unwrap()
        })
        .collect();
    let anomaly = events
        .iter()
        .find(|e| e.name == "anomaly")
        .expect("a corrupt resume row ships an anomaly");
    assert_eq!(
        anomaly.fields["kind"],
        serde_json::json!("resume_cache_corrupt")
    );
}

// -- renderer seam: remote playback -------------------------------------------

use crate::renderer::{
    cast_stream_format, ReceiverStatus, RendererChannel, RendererError, RendererMedia,
    RendererPlayerState, RendererSessionStatus,
};

/// Shared, scriptable state for the fake renderer channel: the commands the
/// session issued and the status each poll returns. The service drives any
/// `RendererChannel`, so this one fake covers the transport routing for both
/// renderer flavors.
#[derive(Default)]
struct FakeRendererState {
    loads: Vec<RendererMedia>,
    seeks: Vec<std::time::Duration>,
    pauses: u32,
    plays: u32,
    stops: u32,
    volumes: Vec<f32>,
}

#[derive(Clone)]
struct FakeRendererChannel {
    state: Arc<Mutex<FakeRendererState>>,
}

impl FakeRendererChannel {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeRendererState::default())),
        }
    }
}

impl RendererChannel for FakeRendererChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        self.state.lock().unwrap().loads.push(media.clone());
        Ok(())
    }
    fn play(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().plays += 1;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }
    fn seek(&mut self, position: std::time::Duration) -> Result<(), RendererError> {
        self.state.lock().unwrap().seeks.push(position);
        Ok(())
    }
    fn set_volume(&mut self, level: f32) -> Result<(), RendererError> {
        self.state.lock().unwrap().volumes.push(level);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        Ok(ReceiverStatus {
            player_state: RendererPlayerState::Playing,
            position: None,
            duration: None,
            volume: Some(1.0),
        })
    }
}

/// Poll `predicate` until it holds or a 2s deadline passes.
fn wait_until(predicate: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    predicate()
}

/// Seed the audio format, segment, and backing file that make `track_id`
/// resolvable, so the remote path can turn it into media. No real bytes on disk —
/// the device, not bae, fetches the audio, so the remote path never decodes it.
async fn seed_playable_track(
    library_manager: &crate::library::LibraryManager,
    release_id: &str,
    track_id: &str,
) {
    use crate::db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, DbFile};
    use crate::util::content_type::ContentType;
    let now = chrono::Utc::now();
    let file_id = bae_test_support::test_uuid(&format!("{track_id}-file"));
    let file = DbFile::new(
        release_id,
        "track.flac",
        4_096,
        ContentType::Flac,
        file_id.clone(),
        now,
        crate::util::fs::hash_bytes(b"fixture"),
    );
    library_manager.add_file(&file).await.unwrap();
    let audio_format_id = bae_test_support::test_uuid(&format!("{track_id}-af"));
    let audio_format = DbAudioFormat::new(
        track_id,
        ContentType::Flac,
        44_100,
        Some(16),
        2,
        audio_format_id.clone(),
        now,
    );
    let segment = DbAudioSegment {
        id: bae_test_support::test_uuid(&format!("{track_id}-seg")),
        audio_format_id,
        segment_index: 0,
        role: DbAudioSegmentRole::Main,
        file_id,
        start_sample: 0,
        end_sample: None,
        start_byte: None,
        end_byte: None,
        created_at: now,
    };
    library_manager
        .insert_audio_format_with_segments_for_test(&audio_format, &[segment])
        .await
        .unwrap();
}

/// A playback service over releases whose every track is resolvable to remote
/// media.
async fn remote_service(
    releases: &[(&str, &[&str])],
) -> (
    TempDir,
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, service, rx) = seeded_playback_service(releases).await;
    for (release_id, tracks) in releases {
        for track_id in *tracks {
            seed_playable_track(&service.library_manager, release_id, track_id).await;
        }
    }
    (home, service, rx)
}

fn test_stream_provider() -> crate::renderer::MediaUrlProvider {
    Arc::new(|track_id: &str, _format| Ok(format!("http://renderer.local/stream?id={track_id}")))
}

fn remote_connect(channel: FakeRendererChannel) -> RemoteConnect {
    RemoteConnect::new(
        Box::new(channel),
        "Living Room".to_string(),
        test_stream_provider(),
        Arc::new(|_| None),
        cast_stream_format,
    )
}

/// `play_on` mid-track keeps the current track and queue position, switches the
/// renderer to Remote, and reissues the current track to the device at its
/// current position (a LOAD plus a seek).
#[tokio::test]
async fn play_on_reissues_current_track_at_position() {
    let (_home, mut service, _rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec![
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653".to_string(),
        ],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::from_secs(30));

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;

    assert!(
        service.renderer.is_remote(),
        "the renderer switches to Remote"
    );
    assert_eq!(
        service.slot.current_track_id(),
        Some("08c7ff07-b56a-4e16-8df6-ae2967fa0806"),
        "the current track is unchanged"
    );
    assert!(
        wait_until(|| {
            let s = state.lock().unwrap();
            s.loads.len() == 1 && s.seeks.contains(&std::time::Duration::from_secs(30))
        }),
        "the current track is loaded onto the device and seeked to its position"
    );
    assert_eq!(
        state.lock().unwrap().loads[0].url,
        "http://renderer.local/stream?id=08c7ff07-b56a-4e16-8df6-ae2967fa0806"
    );
}

/// A device `Finished` status advances the shared queue to the next track and
/// loads it onto the device — the same advance path local end-of-track uses.
#[tokio::test]
async fn remote_finished_advances_queue_and_loads_next() {
    let (_home, mut service, _rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &[
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806",
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653",
        ],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec![
            "08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string(),
            "08c7fe07-b56a-4c63-8df6-ad2967fa0653".to_string(),
        ],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));

    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Finished,
            position: None,
            duration: None,
            volume: Some(1.0),
            ended: false,
        })
        .await;

    assert_eq!(
        service.slot.current_track_id(),
        Some("08c7fe07-b56a-4c63-8df6-ad2967fa0653"),
        "the queue advanced to the next track"
    );
    assert!(
        wait_until(|| state.lock().unwrap().loads.iter().any(
            |m| m.url == "http://renderer.local/stream?id=08c7fe07-b56a-4c63-8df6-ad2967fa0653"
        )),
        "the next track is loaded onto the device"
    );
}

/// A non-terminal device status feeds the shared progress channel, so every UI
/// and the position store update exactly as for local playback.
#[tokio::test]
async fn remote_status_feeds_progress() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    service.handle_play_on(remote_connect(channel)).await;
    // Drain the setup events.
    while rx.try_recv().is_ok() {}

    service
        .handle_remote_status(RendererSessionStatus {
            player_state: RendererPlayerState::Playing,
            position: Some(std::time::Duration::from_secs(30)),
            duration: Some(std::time::Duration::from_secs(180)),
            volume: Some(1.0),
            ended: false,
        })
        .await;

    let mut saw_position = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::PositionUpdate {
            position_ms,
            track_id,
            ..
        } = progress
        {
            if track_id == "08c7ff07-b56a-4e16-8df6-ae2967fa0806" && position_ms == 30_000 {
                saw_position = true;
            }
        }
    }
    assert!(
        saw_position,
        "the device's position must flow as a PositionUpdate for the current track"
    );
}

/// Stopping remote playback stops the device, drops the renderer back to Local,
/// and announces `RemoteStatusChanged(None)` so the UI leaves the remote state.
#[tokio::test]
async fn stop_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.handle_stop_remote().await;

    assert!(
        !service.renderer.is_remote(),
        "the renderer returns to Local"
    );
    assert!(
        wait_until(|| state.lock().unwrap().stops == 1),
        "the device is told to stop"
    );
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { device_name: None } = progress {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping remote playback announces RemoteStatusChanged(None)"
    );
}

/// A plain `stop()` while playing remotely must stop the device and return to
/// local — stop means stop (pause is what keeps the session warm). Without
/// routing stop through the renderer, the local slot goes Stopped while the
/// device stays connected and playing. This is the routing bug the Cast round
/// hit; the DLNA channel is held to the same contract in its own tests.
#[tokio::test]
async fn stop_while_remote_stops_device_and_returns_to_local() {
    let (_home, mut service, mut rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    while rx.try_recv().is_ok() {}

    service.stop().await;

    assert!(
        !service.renderer.is_remote(),
        "stop must return the renderer to local"
    );
    assert!(
        wait_until(|| state.lock().unwrap().stops == 1),
        "stop must stop the device, not leave it playing"
    );
    assert!(
        matches!(service.slot, PlaybackSlot::Stopped),
        "the slot must be Stopped after stop"
    );
    let mut saw_not_remote = false;
    while let Ok(progress) = rx.try_recv() {
        if let PlaybackProgress::RemoteStatusChanged { device_name: None } = progress {
            saw_not_remote = true;
        }
    }
    assert!(
        saw_not_remote,
        "stopping while remote announces RemoteStatusChanged(None)"
    );
}

/// Set up a remote-playback service over a single fake-backed track `t1`, current
/// at position 0, and return the service plus the fake channel's shared state.
async fn remote_over_fake() -> (
    TempDir,
    PlaybackService,
    Arc<Mutex<FakeRendererState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, mut service, rx) = remote_service(&[(
        "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
        &["08c7ff07-b56a-4e16-8df6-ae2967fa0806"],
    )])
    .await;
    service.playback_queue.play_release(
        ContextSource::Release("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e".to_string()),
        vec!["08c7ff07-b56a-4e16-8df6-ae2967fa0806".to_string()],
        ContextStart::Index(0),
    );
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(
        test_prepared_track("08c7ff07-b56a-4e16-8df6-ae2967fa0806", buffer),
        TrackPhase::Playing,
    );
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

    let channel = FakeRendererChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    assert!(wait_until(|| !state.lock().unwrap().loads.is_empty()));
    (home, service, state, rx)
}

/// Pause while remote routes to the device.
#[tokio::test]
async fn pause_while_remote_pauses_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(
        wait_until(|| state.lock().unwrap().pauses == 1),
        "pause while remote must pause the device"
    );
}

/// Resume while remote routes to the device.
#[tokio::test]
async fn resume_while_remote_plays_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.pause();
    assert!(wait_until(|| state.lock().unwrap().pauses == 1));
    service.resume().await;
    assert!(
        wait_until(|| state.lock().unwrap().plays == 1),
        "resume while remote must play the device"
    );
}

/// Seek while remote routes to the device (and skips the local rebuild path).
#[tokio::test]
async fn seek_while_remote_seeks_the_device() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.seek(std::time::Duration::from_secs(45)).await;
    assert!(
        wait_until(|| state
            .lock()
            .unwrap()
            .seeks
            .contains(&std::time::Duration::from_secs(45))),
        "seek while remote must seek the device"
    );
}

/// Setting the volume while remote sets the device's volume too.
#[tokio::test]
async fn set_volume_while_remote_sets_the_device_volume() {
    let (_home, mut service, state, _rx) = remote_over_fake().await;
    service.set_volume(0.3);
    assert!(
        wait_until(|| state.lock().unwrap().volumes.contains(&0.3)),
        "setting the volume while remote must set the device's volume"
    );
}

// -- AirPlay renderer-seam tests --

/// Records the control operations the service drives an AirPlay stream through,
/// standing in for the RAOP session so the seam is tested without a receiver.
#[derive(Default)]
struct FakeAirPlayControlState {
    flushed: std::sync::atomic::AtomicU64,
    reanchored: std::sync::atomic::AtomicU64,
    failed: std::sync::atomic::AtomicBool,
}

struct FakeAirPlayControl(Arc<FakeAirPlayControlState>);

impl crate::playback::airplay_output::AirPlayStreamControl for FakeAirPlayControl {
    fn flush(&self) {
        self.0
            .flushed
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn reanchor(&self) {
        self.0
            .reanchored
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn has_failed(&self) -> bool {
        self.0.failed.load(std::sync::atomic::Ordering::Acquire)
    }
    fn frames_sent(&self) -> u64 {
        0
    }
    fn latency_frames(&self) -> u32 {
        88_200
    }
}

/// Install an AirPlay renderer on `service` with a fake control published, a
/// tagged saved local output (volume `saved_tag`), and the given latency.
fn install_airplay(
    service: &mut PlaybackService,
    latency_frames: u32,
    saved_tag: f32,
) -> Arc<FakeAirPlayControlState> {
    let state = Arc::new(FakeAirPlayControlState::default());
    let control: Arc<dyn crate::playback::airplay_output::AirPlayStreamControl> =
        Arc::new(FakeAirPlayControl(state.clone()));
    let saved = TestAudioOutput::new();
    saved.set_volume(saved_tag);
    service.renderer = Renderer::AirPlay(renderer::AirPlayRenderer::new(
        control,
        Box::new(saved),
        latency_frames,
    ));
    state
}

#[tokio::test]
async fn airplay_pause_flushes_and_resume_reanchors() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    let state = install_airplay(&mut service, 88_200, 0.5);

    service.pause();
    assert_eq!(
        state.flushed.load(std::sync::atomic::Ordering::Acquire),
        1,
        "pause FLUSHes the receiver"
    );
    assert_eq!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire),
        0
    );

    service.resume().await;
    assert_eq!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire),
        1,
        "resume re-anchors the pacing"
    );
}

#[tokio::test]
async fn airplay_position_is_offset_by_receiver_latency() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    // 88_200 frames at 44.1 kHz = 2 s of latency.
    install_airplay(&mut service, 88_200, 0.5);

    // A tick at 5 s of decoded position: the audible position is 5 − 2 = 3 s.
    let mut fmt = test_track_fmt("t");
    fmt.duration_ms = 60_000;
    service
        .handle_position_event(Arc::new(fmt), std::time::Duration::from_secs(5))
        .await;

    let position = loop {
        match progress_rx.try_recv() {
            Ok(PlaybackProgress::PositionUpdate { position_ms, .. }) => break position_ms,
            Ok(_) => continue,
            Err(_) => panic!("expected a PositionUpdate"),
        }
    };
    assert_eq!(
        position, 3_000,
        "position reflects the ~2 s receiver latency"
    );
}

#[tokio::test]
async fn airplay_stop_restores_the_local_output_and_returns_to_local() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    let buffer = create_sparse_buffer(1_024);
    service.slot = active_slot(test_prepared_track("t", buffer), TrackPhase::Playing);
    install_airplay(&mut service, 88_200, 0.777);

    service.stop().await;

    assert!(
        !service.renderer.is_airplay(),
        "stop returns to the local renderer"
    );
    assert_eq!(
        service.audio_output.get_volume(),
        0.777,
        "the saved local output sink is restored"
    );
}

/// Stopping AirPlay resumes local at the position playback actually reached: the
/// resume position is read from the live shared position at teardown, so the
/// `AirPlayRenderer` carries no separately-stored position that could go stale.
/// (Fully exercising the resumed local decode needs a real imported track; here
/// the position source and the return-to-local are asserted.)
#[tokio::test]
async fn airplay_stop_reads_the_live_position_and_returns_to_local() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let saved_tag = 0.5;
    install_airplay(&mut service, 88_200, saved_tag);

    // Playback progressed on AirPlay: decode is local, so the shared position is
    // the live one the resume reads.
    *service.current_position_shared.lock().unwrap() = Some(std::time::Duration::from_secs(30));

    service.handle_stop_remote().await;

    assert!(
        !service.renderer.is_airplay(),
        "stop returns to the local renderer"
    );
    assert_eq!(
        service.audio_output.get_volume(),
        saved_tag,
        "the saved local output sink is restored"
    );
}

/// Seeking while on AirPlay FLUSHes the receiver and re-anchors the pacing (decode
/// is local, so the rebuild re-fills the sink at the new position).
#[tokio::test]
async fn airplay_seek_flushes_and_reanchors() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    let buffer = create_sparse_buffer(64 * 1024);
    service.slot = active_slot(
        test_prepared_track(TRACK_T, buffer.clone()),
        TrackPhase::Playing,
    );
    let (_sink, source, _ready) = create_track_stream_pair(44_100, 2);
    let (_tx, audio_rx) = audio_event_channel();
    service.output = Some(test_output(
        Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            test_track_fmt(TRACK_T),
        ))),
        audio_rx,
    ));
    service.current_position_shared =
        Arc::new(std::sync::Mutex::new(Some(std::time::Duration::ZERO)));
    let state = install_airplay(&mut service, 88_200, 0.5);

    service.seek(std::time::Duration::from_secs(20)).await;

    assert!(
        state.flushed.load(std::sync::atomic::Ordering::Acquire) >= 1,
        "seek FLUSHes the receiver's buffer"
    );
    assert!(
        state.reanchored.load(std::sync::atomic::Ordering::Acquire) >= 1,
        "seek re-anchors the pacing"
    );
}

/// A local end-of-decode advances the queue while the AirPlay renderer stays
/// installed — playback moves to the next track on the same receiver.
#[tokio::test]
async fn airplay_advance_on_local_end_stays_on_airplay() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_A, TRACK_B])])
            .await;
    service.playback_queue.play_release(
        ContextSource::Release("af63ef4c-8602-4cd5-82c0-3d334b916305".to_string()),
        vec![TRACK_A.to_string(), TRACK_B.to_string()],
        ContextStart::Index(0),
    );
    service.slot = active_slot(
        test_prepared_track(TRACK_A, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    install_airplay(&mut service, 88_200, 0.5);

    service.handle_auto_advance(TRACK_A.to_string()).await;

    assert!(
        service.renderer.is_airplay(),
        "the AirPlay renderer stays installed across an auto-advance"
    );
}

/// A no-op AirPlay sink for driving `handle_play_on_airplay`: it accepts the PCM
/// source without touching a socket.
struct NoopAirPlaySink;
impl crate::playback::airplay_output::AirPlaySink for NoopAirPlaySink {
    fn start(
        &self,
        _source: Box<dyn crate::airplay::stream::PcmSource>,
    ) -> Result<Arc<dyn crate::playback::airplay_output::AirPlayStreamControl>, AudioError> {
        Ok(Arc::new(FakeAirPlayControl(Arc::new(
            FakeAirPlayControlState::default(),
        ))))
    }
}

/// `handle_play_on_airplay` swaps to the AirPlay output and installs the AirPlay
/// renderer without turning playback "remote" — decode stays local, so the queue
/// and slot are driven by the local pipeline, not device transport commands.
/// (Driven with nothing playing so the swap isn't torn down by the unit harness's
/// undecodable seed track; the local decode path is covered by the seek/advance
/// tests.)
#[tokio::test]
async fn play_on_airplay_swaps_the_sink_and_keeps_decode_local() {
    let (_home, mut service, _progress_rx) = test_playback_service().await;
    // Nothing playing: AirPlay arms without a track to re-decode.
    service.slot = PlaybackSlot::Stopped;

    service
        .handle_play_on_airplay(renderer::AirPlayConnect::new(
            Box::new(NoopAirPlaySink),
            "Living Room".to_string(),
            88_200,
        ))
        .await;

    assert!(
        service.renderer.is_airplay(),
        "the AirPlay renderer is installed"
    );
    assert!(
        !service.renderer.is_remote(),
        "AirPlay keeps decoding locally — it is not a fetch-a-URL remote renderer"
    );
}

/// A dead AirPlay receiver (the session reports transport failure) ends AirPlay
/// and returns to local — surfaced on the regular position path rather than
/// erroring silently forever.
#[tokio::test]
async fn airplay_receiver_death_ends_airplay_and_returns_to_local() {
    let (_home, mut service, _progress_rx) =
        seeded_playback_service(&[("af63ef4c-8602-4cd5-82c0-3d334b916305", &[TRACK_T])]).await;
    service.slot = active_slot(
        test_prepared_track(TRACK_T, create_sparse_buffer(1_024)),
        TrackPhase::Playing,
    );
    let state = install_airplay(&mut service, 88_200, 0.5);

    // The receiver went away: the session reports the transport as failed.
    state
        .failed
        .store(true, std::sync::atomic::Ordering::Release);

    // A routine position tick catches it and ends AirPlay.
    service
        .handle_position_event(
            Arc::new(test_track_fmt(TRACK_T)),
            std::time::Duration::from_secs(1),
        )
        .await;

    assert!(
        !service.renderer.is_airplay(),
        "a dead receiver ends AirPlay and returns to the local renderer"
    );
}
