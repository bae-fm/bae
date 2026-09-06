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
    let playback_queue = PublishedQueue::new(queue_ids);
    let service = PlaybackService {
        library_manager,
        command_tx,
        command_rx,
        progress_tx,
        playback_queue,
        current_position_shared: Arc::new(std::sync::Mutex::new(None)),
        // These tests drive the handlers directly and never start a preview, so
        // no output is ever opened from this device.
        audio_device: Box::new(crate::playback::audio_output::FailingAudioDevice),
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
        file_buffers: FileBuffers::new(),
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

include!("tests/buffers_and_loading.rs");
include!("tests/starvation_and_start.rs");
include!("tests/queue_and_diagnostics.rs");
include!("tests/remote_output.rs");
