//! The service's test fixtures: a service over a seeded store with a stub
//! output, the slot, track and decoder stand-ins its handlers are driven
//! with, and the fakes remote and AirPlay playback run on.

use super::*;
use crate::audio_output::{
    AudioError, AudioEventReceiver, AudioEventSender, AudioOutputControls, AudioState, AudioStream,
    PlayerOutput,
};
use crate::renderer::channel::{FakeChannel, FakeChannelState};
use crate::renderer::RendererFlavor;
use tempfile::TempDir;

pub(super) struct TestAudioStream;

impl AudioStream for TestAudioStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(())
    }
}

/// A stream whose device reported, once, that it can't go on — what a cpal
/// stream reports when the default device changes on Windows or its device is
/// unplugged.
pub(super) struct InvalidatedTestStream(std::sync::Mutex<Option<String>>);

impl InvalidatedTestStream {
    pub(super) fn new(detail: &str) -> Self {
        Self(std::sync::Mutex::new(Some(detail.to_string())))
    }
}

impl AudioStream for InvalidatedTestStream {
    fn play(&self) -> Result<(), AudioError> {
        Ok(())
    }

    fn take_invalidation(&self) -> Option<String> {
        self.0.lock().unwrap().take()
    }
}

pub(super) struct TestAudioOutput {
    /// Count of device streams built (`create_stream`) vs source swaps
    /// (`on_source_replaced`), so a test can assert the persistent stream is
    /// rebuilt only on a format change, not on a same-format transition.
    pub(super) build_count: Arc<std::sync::atomic::AtomicU64>,
    pub(super) replace_count: Arc<std::sync::atomic::AtomicU64>,
}

impl TestAudioOutput {
    pub(super) fn new() -> Self {
        Self {
            build_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            replace_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

impl AudioOutput for TestAudioOutput {
    fn create_stream(
        &mut self,
        _controls: crate::audio_output::AudioOutputControls,
        _source: Arc<Mutex<source::PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        _audio_events: AudioEventSender,
        _position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        self.build_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Box::new(TestAudioStream))
    }

    fn on_source_replaced(&mut self) {
        self.replace_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// `output` as the main player's, playing at full volume, as a test's
/// service starts it.
pub(super) fn test_player_output(output: impl AudioOutput) -> PlayerOutput {
    let controls = AudioOutputControls::new(crate::Volume::FULL);
    controls.set_state(AudioState::Playing);
    PlayerOutput::new(Box::new(output), controls)
}

/// Track ids for the fixtures that insert real `tracks` rows. coven requires a
/// canonical UUID on every synced-table row, so a seeded track cannot be named
/// `"t"`; the short names elsewhere in these tests are in-memory playback ids that
/// never reach the database. A track the queue plays is a `tracks` row.
pub(super) const TRACK_T: &str = "3e747c7f-879c-44a2-ae1e-9767a8d76b15";

pub(super) const TRACK_A: &str = "bb035a79-2d3b-4ba0-ac99-121564ea97d2";

pub(super) const TRACK_B: &str = "36a9322d-0c07-4223-9c0b-2110a1fdc622";

/// Insert each `(release_id, track_ids)` as album + release + track rows so
/// `get_track_ids` returns each release's tracks in the given order: they are
/// inserted, and so played, in slice order, on the release's one CD. Each
/// stores `track_length_ms` as its length.
pub(super) async fn seed_test_releases(
    db: &bae_store::Store,
    releases: &[(&str, &[&str])],
    track_length_ms: u64,
) {
    let records = bae_records::test_fixtures::test_records(db);
    use bae_records::{DbAlbum, DbArtist, DbRelease, DbTrack};
    if releases.is_empty() {
        return;
    }
    let artist = DbArtist {
        id: bae_util::test_ids::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    };
    records.insert_artist(&artist).await.unwrap();
    for (release_id, track_ids) in releases {
        let album = DbAlbum::new_test("Album Title", &coven::UuidIds);
        let release = DbRelease::new_test(&album.id, release_id);
        records.insert_album(&album).await.unwrap();
        records
            .insert_album_artist(&bae_records::DbAlbumArtist::new_test(&album.id, &artist.id))
            .await
            .unwrap();
        records.insert_release(&release).await.unwrap();
        records
            .insert_release_medium(&bae_records::DbReleaseMedium::new_test(release_id))
            .await
            .unwrap();
        for (index, track_id) in track_ids.iter().enumerate() {
            let mut track = DbTrack::new_test(
                release_id,
                track_id,
                "Track Title",
                u32::try_from(index + 1).expect("a test track number fits u32"),
            );
            track.duration_ms = track_length_ms;
            records.insert_track(&track).await.unwrap();
        }
    }
}

/// Make the seeded `release_id` the playing context from its first track.
pub(super) async fn fill_test_release(service: &mut PlaybackService, release_id: &str) {
    let filled = service
        .playback_queue
        .play(ContextFill::Release {
            release_id: release_id.to_string(),
            start: ReleaseStart::First,
        })
        .await
        .expect("the queue tables take the release");
    assert!(
        matches!(filled, ContextFilled::Playing { .. }),
        "the seeded release has tracks: {filled:?}"
    );
}

/// A store seeded with releases, and the directory holding it, which
/// outlives it.
pub(super) struct SeededStore {
    db: bae_store::Store,
    home: TempDir,
}

impl SeededStore {
    /// The seeded store.
    pub(super) fn store(&self) -> &bae_store::Store {
        &self.db
    }
}

pub(super) async fn seeded_store(releases: &[(&str, &[&str])]) -> SeededStore {
    seeded_store_with_track_length(releases, 180_000).await
}

pub(super) async fn seeded_store_with_track_length(
    releases: &[(&str, &[&str])],
    track_length_ms: u64,
) -> SeededStore {
    let home = TempDir::new().unwrap();
    let db =
        bae_store::Store::new_test(crate::TEST_SCHEMA, home.path(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
    seed_test_releases(&db, releases, track_length_ms).await;
    SeededStore { db, home }
}

pub(super) async fn test_playback_service(
) -> (SeededStore, PlaybackService, tokio_mpsc::UnboundedReceiver<PlaybackProgress>) {
    seeded_playback_service(&[]).await
}

/// A `Diagnostics` wired to a `RecordingTransport`, for emission tests that
/// assert typed events reach the wire. The transport replays success (the
/// drained outcome queue defaults to `Ok`).
pub(super) fn recording_diagnostics(
) -> (bae_diagnostics::Diagnostics, Arc<bae_diagnostics::RecordingTransport>) {
    use bae_diagnostics::{
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
    let diagnostics = Diagnostics::recording_to(
        config,
        Arc::new(coven::SystemClock),
        Arc::new(coven::SequentialIds::new()),
        transport.clone(),
    )
    .expect("diagnostics starts");
    (diagnostics, transport)
}

pub(super) async fn seeded_playback_service(
    releases: &[(&str, &[&str])],
) -> (SeededStore, PlaybackService, tokio_mpsc::UnboundedReceiver<PlaybackProgress>) {
    let store = seeded_store(releases).await;
    let (service, progress_rx) =
        playback_service_over(store.store(), bae_diagnostics::Diagnostics::noop());
    (store, service, progress_rx)
}

/// Assemble a directly-drivable `PlaybackService` over `database` (with a stub audio output),
/// reporting to `diagnostics`, for tests that call its handlers without the actor loop: no
/// loop takes the commands it sends itself.
pub(super) fn playback_service_over(
    db: &bae_store::Store,
    diagnostics: bae_diagnostics::Diagnostics,
) -> (PlaybackService, tokio_mpsc::UnboundedReceiver<PlaybackProgress>) {
    let (service, _commands, progress_rx) = playback_service_and_commands_over(db, diagnostics);
    (service, progress_rx)
}

/// [`playback_service_over`] with the commands its loop (`PlaybackService::run`) serves.
pub(super) fn playback_service_and_commands_over(
    db: &bae_store::Store,
    diagnostics: bae_diagnostics::Diagnostics,
) -> (
    PlaybackService,
    tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (command_tx, command_rx) = OwnCommands::channel();
    let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
    let preview = PreviewPlayer::new(
        tokio::runtime::Handle::current(),
        progress_tx.clone(),
        command_tx.clone(),
        50,
        diagnostics.clone(),
    );
    let playback_queue = PlaybackQueue::new(crate::tables::PlaybackQueueTables::new(db.clone()));
    let staged_sides = advance::track_infos_follow(db, "the staged crossing's sides", &diagnostics);
    let held_tracks = advance::track_infos_follow(db, "the tracks playback holds", &diagnostics);
    let service = PlaybackService {
        runtime: tokio::runtime::Handle::current(),
        store: db.clone(),
        records: bae_records::test_fixtures::test_records(db),
        people: bae_people::People::new(db.clone()),
        ids: Arc::new(coven::UuidIds),
        diagnostics,
        command_tx,
        progress_tx,
        playback_queue,
        // The desktop's: a test of the gate sets a system that answers.
        audio_focus: crate::AudioFocus::Unshared,
        // These tests drive the handlers directly; a preview that starts a
        // stream gets an output from this device that can't build one.
        audio_device: Arc::new(crate::audio_output::FailingAudioDevice),
        audio_output: test_player_output(TestAudioOutput::new()),
        output: None,
        slot: PlaybackSlot::Stopped,
        listening: None,
        buffered_listening: Vec::new(),
        audible_completion: None,
        load_generation_counter: 0,
        preloaded_next: None,
        volume: OutputVolume::new(),
        preview,
        position_update_interval_ms: 50,
        file_buffers: FileBuffers::new(
            tokio::runtime::Handle::current(),
            FetchArbiter::new(),
            tokio::sync::watch::channel(None).0,
        ),
        starvation_episode: None,
        persisted_position: None,
        first_audio_pending: None,
        renderer: Renderer::Local,
        clock: Arc::new(crate::ManualPlaybackClock::new(test_clock_start())),
        staged_sides,
        held_tracks,
    };
    (service, command_rx, progress_rx)
}

/// Where a test's playback clock starts.
pub(super) fn test_clock_start() -> chrono::DateTime<chrono::Utc> {
    "2026-01-01T00:00:00Z".parse().unwrap()
}

/// An already-exited decoder with a fresh token — the current track's decoder for
/// slot-shaped tests that don't exercise a live decode.
pub(super) fn test_decoder() -> TrackDecoder {
    TrackDecoder {
        handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

/// Build a persistent `OutputStream` over `source` with a stub device stream and
/// the given audio-events receiver, for tests that drive the drain / seek paths.
pub(super) fn test_output(
    source: Arc<Mutex<source::PlaybackSource>>,
    audio_events: AudioEventReceiver,
) -> OutputStream {
    OutputStream {
        stream: Box::new(TestAudioStream),
        source,
        audio_events,
        sample_rate: 44_100,
        channels: 2,
    }
}

/// Build an `Active` slot from a prepared track with a stub decoder. Tests that
/// exercise the persistent output set `service.output` separately.
pub(super) fn active_slot(prepared: PlaybackPreparedTrack, phase: TrackPhase) -> PlaybackSlot {
    PlaybackSlot::Active(CurrentTrack {
        prepared,
        decoder: test_decoder(),
        phase,
        position: StreamPosition::START,
    })
}

pub(super) fn test_prepared_track(
    track_id: &str,
    buffer: SharedSparseBuffer,
) -> PlaybackPreparedTrack {
    test_prepared_track_with_file(track_id, track_id, buffer)
}

pub(super) fn test_prepared_track_with_file(
    track_id: &str,
    file_id: &str,
    buffer: SharedSparseBuffer,
) -> PlaybackPreparedTrack {
    PlaybackPreparedTrack {
        track_id: track_id.to_string(),
        segments: vec![PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: file_id.to_string(),
            buffer,
            span: bae_records::SegmentSpan::whole_file(),
            scrub_points: bae_records::ScrubPoints::new(Vec::new()),
        }],
        sample_rate: 44_100,
        channels: 2,
        generated_pregap_frames: 0,
        timeline: TrackTimeline::new(
            Some(std::time::Duration::from_secs(1)),
            std::time::Duration::ZERO,
        ),
        content_type: bae_util::content_type::ContentType::Flac,
        replay_gain_linear: 1.0,
    }
}

pub(super) fn finished_decoder_handle() -> bae_diagnostics::ReportedThread {
    bae_diagnostics::Diagnostics::noop()
        .spawn_thread_reported("finished decoder", || {}, |_| {})
        .expect("spawn a decoder thread that finishes at once")
}

/// A preloaded next track whose decoder has already finished, holding a cancel
/// token nothing else can observe. A test that reads the token back passes its
/// own in with struct-update syntax.
pub(super) fn test_preloaded_next(
    prepared: PlaybackPreparedTrack,
    source: PreloadedNextSource,
) -> PreloadedNext {
    PreloadedNext {
        prepared,
        decoder_handle: finished_decoder_handle(),
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source,
    }
}

/// Run `decode` to its end, as playback's decoder thread does, and return the
/// samples it played.
pub(super) fn play_decode(
    decode: bae_track_audio::StreamDecodeParams,
    sample_rate: u32,
    channels: u32,
) -> Vec<f32> {
    let (mut sink, mut stream, _ready) =
        bae_audio::track_stream::create_track_stream_pair_with_capacity(
            sample_rate,
            channels,
            4 * sample_rate as usize,
        );
    let decoder = std::thread::spawn(move || {
        decode.run_decoder(&mut sink, Arc::new(std::sync::atomic::AtomicBool::new(false)))
    });
    let mut played = Vec::new();
    let mut chunk = [0.0f32; 4_096];
    loop {
        let pulled = stream.pull_samples(&mut chunk);
        played.extend_from_slice(&chunk[..pulled]);
        if pulled == 0 && stream.is_finished() {
            break;
        }
    }
    decoder.join().expect("the decoder thread").expect("the decode runs to the window's end");
    played
}

/// The release the loading tests play, and the tracks on it.
pub(super) const LOADING_RELEASE: &str = "0c9b6d3e-2f41-4e8a-9b57-3d1a6e8f2c40";

pub(super) const FINISHED_TRACK: &str = "1d8e4a72-93b5-4c06-8f21-7a3c5e9d0b18";

pub(super) const INCOMING_TRACK: &str = "2e7f5b83-a4c6-4d17-9032-8b4d6fa01c29";

pub(super) const NEXT_TRACK: &str = "3f806c94-b5d7-4e28-a143-9c5e70b12d3a";

/// Whether any `PlaybackError` reached the UI. A read failure that surfaces one
/// halts playback (the progress self-subscription turns it into `HaltOnError`),
/// so its absence is what says the playing track was left alone.
pub(super) fn drained_a_playback_error(
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

pub(super) fn drained_track_completed_ids(
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

/// `current` paused at the end of its side, resuming into `next`.
pub(super) fn side_paused_between(
    current: &str,
    next: &str,
    resumes_at: Option<chrono::DateTime<chrono::Utc>>,
) -> PlaybackSlot {
    let buffer = create_sparse_buffer(1_024);
    active_slot(
        test_prepared_track(current, buffer),
        TrackPhase::Paused(PausePhase::SideEnded(SidePauseDecision {
            track_id: next.to_string(),
            boundary: SideBoundary {
                id: format!("{next}:1:Vinyl"),
                kind: PlaybackPauseBoundary::Side,
                side_label: "A".to_string(),
            },
            resumes_at,
        })),
    )
}

/// Every state the service emitted since the last drain.
pub(super) fn drained_states(
    progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Vec<PlaybackState> {
    let mut states = Vec::new();
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::StateChanged { state } = progress {
            states.push(state);
        }
    }
    states
}

pub(super) fn countdown_deadline() -> chrono::DateTime<chrono::Utc> {
    test_clock_start() + chrono::Duration::seconds(5)
}

/// Wait until `predicate` holds of the fake channel's state, woken by each
/// command the session applies.
pub(super) fn wait_until(
    state: &Mutex<FakeChannelState>,
    predicate: impl Fn(&FakeChannelState) -> bool,
) -> bool {
    crate::renderer::channel::wait_for_state(state, predicate)
}

/// Seed the audio format, segment, and backing file that make `track_id`
/// resolvable, so the remote path can turn it into media. No real bytes on disk —
/// the device, not bae, fetches the audio, so the remote path never decodes it.
pub(super) async fn seed_playable_track(store: &SeededStore, release_id: &str, track_id: &str) {
    seed_track_window(store, release_id, track_id, 0, None, None).await;
}

/// [`seed_playable_track`] with its file stored as `content_type` in
/// `container`.
pub(super) async fn seed_playable_track_stored_as(
    store: &SeededStore,
    release_id: &str,
    track_id: &str,
    content_type: bae_util::content_type::ContentType,
    container: bae_util::audio_container::AudioContainer,
) {
    seed_track(store, release_id, track_id, (content_type, container), (0, None, None)).await;
}

/// [`seed_playable_track`] with its one main segment spanning
/// `start_sample..end_sample` of its file, as a CUE image's track does, after
/// `generated_pregap_samples` of silence a CUE `PREGAP` generates.
pub(super) async fn seed_track_window(
    store: &SeededStore,
    release_id: &str,
    track_id: &str,
    start_sample: u64,
    end_sample: Option<u64>,
    generated_pregap_samples: Option<u64>,
) {
    use bae_util::audio_container::AudioContainer;
    use bae_util::content_type::ContentType;
    seed_track(
        store,
        release_id,
        track_id,
        (ContentType::Flac, AudioContainer::Flac),
        (start_sample, end_sample, generated_pregap_samples),
    )
    .await;
}

/// Seed `track_id`'s file, stored as `(codec, container)`, and its one main
/// segment spanning `(start_sample, end_sample)` after
/// `generated_pregap_samples` of generated silence.
async fn seed_track(
    store: &SeededStore,
    release_id: &str,
    track_id: &str,
    (content_type, container): (
        bae_util::content_type::ContentType,
        bae_util::audio_container::AudioContainer,
    ),
    (start_sample, end_sample, generated_pregap_samples): (u64, Option<u64>, Option<u64>),
) {
    use bae_records::source_audio::{SourceAudioFile, SourceAudioLayout};
    use bae_records::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, NewReleaseFile};
    let now = chrono::Utc::now();
    let lossless = content_type.is_lossless_audio();
    let file_id = bae_util::test_ids::test_uuid(&format!("{track_id}-file"));
    let mut file = NewReleaseFile::new(
        release_id,
        &format!("track.{}", content_type.file_extension()),
        content_type.clone(),
        file_id.clone(),
        now,
    );
    file.source_audio = Some(SourceAudioFile {
        layout: Some(SourceAudioLayout::File),
        format: bae_types::audio_format::AudioFormat {
            codec: content_type.display_name().to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: lossless.then_some(16),
            bitrate_kbps: (!lossless).then_some(128),
            channels: 2,
        },
        content_type: content_type.clone(),
        container,
        duration_ms: 180_000,
    });
    bae_records::test_fixtures::aggregate_ordering::insert_file(
        store.store(),
        store.home.path(),
        file,
        4_096,
    )
    .await;
    let audio_format_id = bae_util::test_ids::test_uuid(&format!("{track_id}-af"));
    let audio_format = DbAudioFormat::new(
        track_id,
        content_type,
        std::num::NonZeroU32::new(44_100).unwrap(),
        lossless.then(|| std::num::NonZeroU32::new(16).unwrap()),
        std::num::NonZeroU32::new(2).unwrap(),
        audio_format_id.clone(),
        now,
    )
    .with_pregaps(None, generated_pregap_samples);
    let segment = DbAudioSegment {
        id: bae_util::test_ids::test_uuid(&format!("{track_id}-seg")),
        audio_format_id,
        segment_index: 0,
        role: DbAudioSegmentRole::Main,
        file_id,
        start_sample,
        end_sample,
        start_byte: None,
        end_byte: None,
        created_at: now,
    };
    bae_records::test_fixtures::test_records(&store.db)
        .insert_audio_format_with_segments_for_test(&audio_format, &[segment])
        .await
        .unwrap();
}

/// A playback service over releases whose every track is resolvable to remote
/// media.
pub(super) async fn remote_service(
    releases: &[(&str, &[&str])],
) -> (SeededStore, PlaybackService, tokio_mpsc::UnboundedReceiver<PlaybackProgress>) {
    let (home, service, rx) = seeded_playback_service(releases).await;
    for (release_id, tracks) in releases {
        for track_id in *tracks {
            seed_playable_track(&home, release_id, track_id).await;
        }
    }
    (home, service, rx)
}

pub(super) fn test_stream_provider() -> crate::renderer::MediaUrlProvider {
    Arc::new(|track_id: &str, _format| Ok(format!("http://renderer.local/stream?id={track_id}")))
}

pub(super) fn test_device() -> crate::RemoteDevice {
    crate::RemoteDevice {
        id: "renderer-device-id".to_string(),
        name: "Speaker Name".to_string(),
        kind: crate::renderer::RendererKind::Cast,
    }
}

pub(super) fn remote_connect(channel: FakeChannel) -> RemoteConnect {
    RemoteConnect::new(
        Box::new(channel),
        test_device(),
        crate::renderer::RendererMediaSource::new(
            test_stream_provider(),
            Arc::new(|cover| {
                format!("http://renderer.local/cover?id={}&v={}", cover.id, cover.version)
            }),
            RendererFlavor::Cast,
        ),
    )
}

/// The arrange every remote test shares: a service over one release of `tracks`,
/// that release playing from its first track, and the session handed to a fake
/// device by `handle_play_on`, from `position` in the first track's stream.
/// Returns the service, the fake channel's shared state and the progress
/// receiver.
pub(super) async fn playing_remote_fixture(
    tracks: &[&str],
    position: StreamPosition,
) -> (
    SeededStore,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    remote_fixture_from(tracks, position, TrackPhase::Playing).await
}

/// [`playing_remote_fixture`] with the first track in `phase` when it is
/// handed to the device.
pub(super) async fn remote_fixture_from(
    tracks: &[&str],
    position: StreamPosition,
    phase: TrackPhase,
) -> (
    SeededStore,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    const RELEASE: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
    let (home, mut service, rx) = remote_service(&[(RELEASE, tracks)]).await;
    fill_test_release(&mut service, RELEASE).await;
    service.slot = active_slot(test_prepared_track(tracks[0], create_sparse_buffer(1_024)), phase);
    if let PlaybackSlot::Active(cur) = &mut service.slot {
        cur.position = position;
    }

    let channel = FakeChannel::new();
    let state = channel.state.clone();
    service.handle_play_on(remote_connect(channel)).await;
    (home, service, state, rx)
}

/// [`playing_remote_fixture`] over one track at position zero, waited until the
/// device has taken its first load.
pub(super) async fn remote_over_fake() -> (
    SeededStore,
    PlaybackService,
    Arc<Mutex<FakeChannelState>>,
    tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    let (home, service, state, rx) =
        playing_remote_fixture(&["08c7ff07-b56a-4e16-8df6-ae2967fa0806"], StreamPosition::START)
            .await;
    assert!(wait_until(&state, |s| !s.loads.is_empty()));
    (home, service, state, rx)
}

/// Records the control operations the service drives an AirPlay stream through,
/// standing in for the RAOP session so the seam is tested without a receiver.
#[derive(Default)]
pub(super) struct FakeAirPlayControlState {
    pub(super) flushed: std::sync::atomic::AtomicU64,
    pub(super) reanchored: std::sync::atomic::AtomicU64,
    pub(super) failure: std::sync::OnceLock<String>,
}

pub(super) struct FakeAirPlayControl {
    pub(super) state: Arc<FakeAirPlayControlState>,
    pub(super) latency: std::time::Duration,
}

impl crate::airplay_output::AirPlayStreamControl for FakeAirPlayControl {
    fn flush(&self) {
        self.state.flushed.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn reanchor(&self) {
        self.state.reanchored.fetch_add(1, std::sync::atomic::Ordering::Release);
    }
    fn failure(&self) -> Option<String> {
        self.state.failure.get().cloned()
    }
    fn latency(&self) -> std::time::Duration {
        self.latency
    }
}

/// A fake AirPlay session bound as the live one: what it recorded, the
/// session itself, which the test holds the way an output stream does, and
/// the saved local output, by where it lives.
#[must_use = "the session is live only while this is held"]
pub(super) struct InstalledAirPlay {
    pub(super) state: Arc<FakeAirPlayControlState>,
    _session: Arc<dyn crate::airplay_output::AirPlayStreamControl>,
    saved_output: AudioOutputControls,
}

impl InstalledAirPlay {
    /// Whether `output` is the local output AirPlay saved.
    pub(super) fn is_saved_output(&self, output: &PlayerOutput) -> bool {
        output.plays_by(&self.saved_output)
    }
}

/// Install an AirPlay renderer on `service` with a saved local output and a
/// fake session at the receiver `latency` bound as the live one.
pub(super) fn install_airplay(
    service: &mut PlaybackService,
    latency: std::time::Duration,
) -> InstalledAirPlay {
    let state = Arc::new(FakeAirPlayControlState::default());
    let session: Arc<dyn crate::airplay_output::AirPlayStreamControl> =
        Arc::new(FakeAirPlayControl { state: state.clone(), latency });
    let control = Arc::new(crate::airplay_output::AirPlayControl::new());
    control.bind(&session);
    let saved_output = AudioOutputControls::new(crate::Volume::FULL);
    saved_output.set_state(AudioState::Playing);
    let saved = PlayerOutput::new(Box::new(TestAudioOutput::new()), saved_output.clone());
    service.renderer =
        Renderer::AirPlay(renderer::AirPlayRenderer::new(test_device(), control, saved));
    InstalledAirPlay { state, _session: session, saved_output }
}

fn test_resolved_track_audio(track_id: &str) -> ResolvedTrackAudio {
    ResolvedTrackAudio {
        track_id: track_id.to_string(),
        release_id: "release-id".to_string(),
        segments: vec![],
        duration_ms: 1000,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        bits_per_sample: Some(16),
        content_type: bae_util::content_type::ContentType::Flac,
        track_loudness_lufs: None,
        track_peak_linear: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
    }
}

/// INDEX 01 lands on the frame after the pregap's last, whatever the pregap's
/// length in nanoseconds: 88 201 frames at 44.1 kHz is no whole number of them.
#[test]
fn a_direct_start_lands_on_the_frame_after_the_pregap() {
    let mut resolved = test_resolved_track_audio("track");
    for pregap in [88_200, 88_201, 88_199, 1] {
        resolved.pregap_samples = Some(pregap);
        let prepared = finalize_playback_track(
            "track".to_string(),
            &resolved,
            vec![],
            bae_preferences::ReplayGainMode::Off,
        );
        assert_eq!(
            prepared.timeline.track_start().frames(prepared.sample_rate),
            pregap,
            "a pregap of {pregap} frames"
        );
    }
}

/// A direct start lands past a positive pregap, and at the stream's start
/// without one; a natural start always at the stream's start.
#[test]
fn track_start_position_cases() {
    use std::time::Duration;
    let pregapped =
        TrackTimeline::new(Some(Duration::from_secs(60)), std::time::Duration::from_millis(3000));
    let no_pregap = TrackTimeline::new(Some(Duration::from_secs(60)), std::time::Duration::ZERO);

    assert_eq!(TrackStart::Direct.position(pregapped), StreamPosition::from_millis(3000));
    assert_eq!(TrackStart::Direct.position(no_pregap), StreamPosition::START);
    assert_eq!(TrackStart::Natural.position(pregapped), StreamPosition::START);
    assert_eq!(
        TrackStart::Position(StreamPosition::from_millis(42_000)).position(pregapped),
        StreamPosition::from_millis(42_000)
    );
}

/// A CUE `PREGAP` directive's silence has no stored pregap behind it; the
/// track's timeline still starts past it, so the seek bar, a direct start and
/// a seek all agree on where INDEX 01 is.
#[test]
fn a_generated_pregap_starts_the_track_past_it() {
    let mut resolved = test_resolved_track_audio("track");
    resolved.generated_pregap_samples = Some(88_200);

    let prepared = finalize_playback_track(
        "track".to_string(),
        &resolved,
        Vec::new(),
        bae_preferences::ReplayGainMode::Off,
    );

    assert_eq!(prepared.generated_pregap_frames, 88_200);
    assert_eq!(prepared.timeline.track_start(), StreamPosition::from_millis(2000));
}

/// A stored pregap is counted in samples; its length on the track's timeline
/// comes from them and the sample rate.
#[test]
fn a_stored_pregap_is_as_long_as_its_samples() {
    let mut resolved = test_resolved_track_audio("track");
    resolved.pregap_samples = Some(66_150);

    let prepared = finalize_playback_track(
        "track".to_string(),
        &resolved,
        Vec::new(),
        bae_preferences::ReplayGainMode::Off,
    );

    assert_eq!(prepared.generated_pregap_frames, 0);
    assert_eq!(prepared.timeline.track_start(), StreamPosition::from_millis(1500));
}

/// A track with both a generated pregap and an audio-pregap segment before its
/// main body, returned with its two buffers so a test can tell which segment
/// decoding starts from.
fn prepared_track_with_pregap_segments(
) -> (PlaybackPreparedTrack, SharedSparseBuffer, SharedSparseBuffer) {
    let pregap_buffer = create_sparse_buffer(1_024);
    let main_buffer = create_sparse_buffer(2_048);
    let mut prepared = test_prepared_track("track", main_buffer.clone());
    prepared.generated_pregap_frames = 441;
    prepared.timeline = TrackTimeline::new(
        Some(std::time::Duration::from_secs(1)),
        std::time::Duration::from_millis(1010),
    );
    prepared.segments = vec![
        PreparedAudioSegment {
            role: DbAudioSegmentRole::AudioPregap,
            file_id: "pregap-file".to_string(),
            buffer: pregap_buffer.clone(),
            span: bae_records::SegmentSpan {
                start_sample: 1_000,
                end_sample: None,
                start_byte: Some(100),
                end_byte: None,
            },
            scrub_points: bae_records::ScrubPoints::new(Vec::new()),
        },
        PreparedAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "main-file".to_string(),
            buffer: main_buffer.clone(),
            span: bae_records::SegmentSpan {
                start_sample: 44_100,
                end_sample: Some(88_200),
                start_byte: Some(2_000),
                end_byte: Some(4_000),
            },
            scrub_points: bae_records::ScrubPoints::new(Vec::new()),
        },
    ];
    (prepared, pregap_buffer, main_buffer)
}

#[test]
fn direct_start_skips_audio_and_generated_pregap_segments() {
    let (prepared, _pregap_buffer, main_buffer) = prepared_track_with_pregap_segments();

    let decode = prepared.decode_params(0, false);

    assert_eq!(decode.leading_silence_frames(), 0);
    assert_eq!(decode.segment_count(), 1);
    assert_eq!(decode.segment_buffer_id(0), main_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 44_100);
    assert_eq!(decode.segment_decode_start(0), bae_audio::DecodeStart::Byte(2_000));
}

#[test]
fn natural_start_includes_audio_and_generated_pregap_segments() {
    let (prepared, pregap_buffer, _main_buffer) = prepared_track_with_pregap_segments();

    let decode = prepared.decode_params(0, true);

    assert_eq!(decode.leading_silence_frames(), 441);
    assert_eq!(decode.segment_count(), 2);
    assert_eq!(decode.segment_buffer_id(0), pregap_buffer.id());
    assert_eq!(decode.segment_target_sample(0), 1_000);
    assert_eq!(decode.segment_decode_start(0), bae_audio::DecodeStart::Byte(100));
}
