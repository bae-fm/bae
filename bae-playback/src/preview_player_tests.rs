use super::*;
use crate::audio_output::{
    audio_event_channel, AudioOutput, FailingAudioDevice, FailingAudioOutput,
};
use crate::capture_output::{
    AudioClock, CaptureAudioDevice, ManualCaptureAudioOutput, ManualCaptureDevice,
};
use crate::service::PlaybackMessage;
use bae_audio::test_samples::{filled_sparse_buffer, samples_as_f32};
use std::sync::Arc;

/// A one-second stereo preview of `preview.wav` over `buffer` — the shape every
/// `start_streaming` test drives.
fn preview_source(buffer: SharedSparseBuffer) -> PreviewSource {
    PreviewSource {
        target: PreviewTarget::whole_file("preview.wav".into()),
        duration: Duration::from_secs(1),
        sample_rate: 44_100,
        channels: 2,
        buffer,
    }
}

#[tokio::test]
async fn preview_play_rejects_unprobeable_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("not-audio.bin");
    std::fs::write(&path, b"not audio").unwrap();
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, _capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);

    let started = player.play(PreviewTarget::whole_file(path.clone()), &device).await.started();
    if started {
        assert_eq!(player.stop(), AfterPreview::LeaveMain);
    }

    assert!(!started);
}

/// Switching to another preview keeps the resume the first one earned: the main
/// player stays paused across the switch, so ending the second preview is what
/// resumes it. A preview that never paused the main player leaves it alone.
#[tokio::test]
async fn switching_previews_keeps_the_main_player_resume() {
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, _capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);
    let path = bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac");
    let sample_rate = u64::from(
        bae_audio::probe_audio_from_path(&path).expect("probe fixture").sample_rate.get(),
    );

    let first = PreviewTarget::sample_range(path.clone(), 0, Some(sample_rate));
    assert!(player.play(first, &device).await.started());
    player.main_player_paused();

    let second = PreviewTarget::sample_range(path, 10 * sample_rate, Some(11 * sample_rate));
    assert!(player.play(second, &device).await.started());

    assert_eq!(player.stop(), AfterPreview::ResumeMain);
    assert_eq!(player.stop(), AfterPreview::LeaveMain, "the resume is handed back once");
}

#[tokio::test]
async fn preview_plays_only_the_requested_sample_window() {
    let fixture_dir = bae_audio::test_fixtures::cue_flac_dir();
    let path = fixture_dir.join("Test Album.flac");
    let probe = bae_audio::probe_audio_from_path(&path).expect("probe fixture");
    let sample_rate = u64::from(probe.sample_rate.get());
    let target =
        PreviewTarget::sample_range(path.clone(), 10 * sample_rate, Some(11 * sample_rate));

    let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, mut capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);

    assert!(player.play(target.clone(), &device).await.started());

    let mut playing = None;
    while let Ok(progress) = progress_rx.try_recv() {
        if let PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
            target,
            duration_ms,
        }) = progress
        {
            playing = Some((target, duration_ms));
        }
    }
    assert_eq!(playing, Some((target, 1_000)));

    let mut captured = capture_rx.recv().await.expect("preview capture");
    let completed = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, command_rx.recv())
        .await
        .expect("the bounded preview completes")
        .expect("bounded preview sends its completion command");
    assert!(matches!(
        completed,
        PlaybackMessage::Own { command: PlaybackCommand::PreviewCompleted, .. }
    ));
    let captured = captured.samples();
    assert_eq!(
        captured.len(),
        probe.sample_rate.get() as usize * probe.channels.get() as usize,
        "a one-second source window emits exactly one second of samples"
    );
    let reference_path = fixture_dir.join("02 Test Artist - Track Two (White Noise).flac");
    let reference_bytes = std::fs::read(reference_path).expect("read reference fixture");
    let reference_buffer = filled_sparse_buffer(&reference_bytes);
    let reference =
        bae_audio::decode_audio(reference_buffer, None, None).expect("decode reference fixture");
    let expected: Vec<f32> = samples_as_f32(&reference).into_iter().take(1_000).collect();
    assert!(captured.len() >= expected.len());
    for (index, (actual, expected)) in captured.iter().zip(&expected).enumerate() {
        assert!(
            (actual - expected).abs() < 0.01,
            "preview sample {index} differs: {actual} vs {expected}"
        );
    }

    assert_eq!(player.stop(), AfterPreview::LeaveMain);
}

#[tokio::test]
async fn preview_seek_is_relative_to_the_requested_sample_window() {
    let fixture_dir = bae_audio::test_fixtures::cue_flac_dir();
    let path = fixture_dir.join("Test Album.flac");
    let probe = bae_audio::probe_audio_from_path(&path).expect("probe fixture");
    let sample_rate = u64::from(probe.sample_rate.get());
    let target =
        PreviewTarget::sample_range(path.clone(), 10 * sample_rate, Some(11 * sample_rate));
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, mut capture_rx) = tokio_mpsc::unbounded_channel();
    let device = ManualCaptureDevice::new(captured, AudioClock::new());

    assert!(player.play(target, &device).await.started());
    player.toggle_pause();
    let _initial_capture = capture_rx.recv().await.expect("initial preview capture");

    player.seek_by_ratio(0.5, &device).await;
    let mut seeked_capture = capture_rx.recv().await.expect("seeked preview capture");
    player.toggle_pause();
    let captured = seeked_capture.wait_for_samples(1_000).await;

    let reference_path = fixture_dir.join("02 Test Artist - Track Two (White Noise).flac");
    let reference_bytes = std::fs::read(reference_path).expect("read reference fixture");
    let reference_buffer = filled_sparse_buffer(&reference_bytes);
    let reference =
        bae_audio::decode_audio(reference_buffer, None, None).expect("decode reference fixture");
    let half_second = probe.sample_rate.get() as usize * probe.channels.get() as usize / 2;
    let expected: Vec<f32> =
        samples_as_f32(&reference).into_iter().skip(half_second).take(1_000).collect();
    assert!(captured.len() >= expected.len());
    for (index, (actual, expected)) in captured.iter().zip(&expected).enumerate() {
        assert!(
            (actual - expected).abs() < 0.01,
            "seeked preview sample {index} differs: {actual} vs {expected}"
        );
    }

    assert_eq!(player.stop(), AfterPreview::LeaveMain);
}

/// A capture output whose streams report, once `report` is set, that their
/// device stopped playing them — what a cpal stream reports when the default
/// device changes on Windows or its device is unplugged.
struct InvalidatingOutput {
    capture: ManualCaptureAudioOutput,
    report: Arc<std::sync::Mutex<Option<String>>>,
}

struct InvalidatingStream {
    capture: Box<dyn crate::AudioStream>,
    report: Arc<std::sync::Mutex<Option<String>>>,
}

impl crate::AudioStream for InvalidatingStream {
    fn play(&self) -> Result<(), crate::AudioError> {
        self.capture.play()
    }

    fn take_invalidation(&self) -> Option<String> {
        self.report.lock().unwrap().take()
    }
}

impl AudioOutput for InvalidatingOutput {
    fn create_stream(
        &mut self,
        controls: crate::audio_output::AudioOutputControls,
        source: Arc<std::sync::Mutex<crate::source::PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: crate::audio_output::AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn crate::AudioStream>, crate::AudioError> {
        let capture = self.capture.create_stream(
            controls,
            source,
            source_sample_rate,
            source_channels,
            audio_events,
            position_update_interval_ms,
        )?;
        Ok(Box::new(InvalidatingStream { capture, report: self.report.clone() }))
    }
}

/// A preview whose stream's device reports it can't go on starts again on a
/// new stream from where it got to — not from the window's start — and a
/// paused preview stays paused. Sabotage: restart at `StreamPosition::START`,
/// and the new stream plays the window's first samples again.
#[tokio::test]
async fn an_invalidated_preview_starts_again_where_it_got_to() {
    let fixture_dir = bae_audio::test_fixtures::cue_flac_dir();
    let path = fixture_dir.join("Test Album.flac");
    let probe = bae_audio::probe_audio_from_path(&path).expect("probe fixture");
    let sample_rate = u64::from(probe.sample_rate.get());
    let target =
        PreviewTarget::sample_range(path.clone(), 10 * sample_rate, Some(11 * sample_rate));
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, mut capture_rx) = tokio_mpsc::unbounded_channel();
    let capture = ManualCaptureAudioOutput::new(captured, AudioClock::new());
    let report = Arc::new(std::sync::Mutex::new(None));
    player.audio_output = Some(crate::audio_output::PlayerOutput::new(
        Box::new(InvalidatingOutput { capture, report: report.clone() }),
        crate::audio_output::AudioOutputControls::new(crate::Volume::FULL),
    ));
    // The preview reuses the output set above, so nothing is opened from this.
    let device = FailingAudioDevice;

    assert!(player.play(target, &device).await.started());
    let mut first_capture = capture_rx.recv().await.expect("the first stream's capture");
    first_capture.wait_for_samples(4_000).await;
    player.toggle_pause();

    *report.lock().unwrap() = Some("the default device changed".to_string());
    let detail = player.take_output_invalidation().expect("the stream's report reaches the player");
    assert_eq!(detail, "the default device changed");
    assert!(player.take_output_invalidation().is_none(), "a report is taken once");
    player.restart_on_current_device(&device).await;

    let mut second_capture = capture_rx.recv().await.expect("a new stream");
    let restarted_at = player.active.as_ref().expect("the preview plays on").pipeline.position();
    assert!(
        restarted_at > crate::StreamPosition::START,
        "the preview had played some of the window"
    );
    assert_eq!(
        player.audio_output.as_ref().unwrap().get_state(),
        AudioState::Paused,
        "a paused preview stays paused"
    );

    player.toggle_pause();
    let captured = second_capture.wait_for_samples(1_000).await;
    let reference_path = fixture_dir.join("02 Test Artist - Track Two (White Noise).flac");
    let reference_bytes = std::fs::read(reference_path).expect("read reference fixture");
    let reference_buffer = filled_sparse_buffer(&reference_bytes);
    let reference =
        bae_audio::decode_audio(reference_buffer, None, None).expect("decode reference fixture");
    let played_samples =
        restarted_at.frames(probe.sample_rate.get()) as usize * probe.channels.get() as usize;
    let expected: Vec<f32> =
        samples_as_f32(&reference).into_iter().skip(played_samples).take(1_000).collect();
    assert!(captured.len() >= expected.len());
    for (index, (actual, expected)) in captured.iter().zip(&expected).enumerate() {
        assert!(
            (actual - expected).abs() < 0.01,
            "restarted preview sample {index} differs: {actual} vs {expected}"
        );
    }

    assert_eq!(player.stop(), AfterPreview::LeaveMain);
}

/// A failed stream start leaves no active preview and cancels the buffer — the
/// preview side of the shared unit's failure contract (the unit cancels the
/// decoder; the owner cancels the buffer). The same start over a device whose
/// outputs work leaves `active` populated with the file as the current path.
#[tokio::test]
async fn failed_preview_stream_start_leaves_no_active_preview() {
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let buffer = create_sparse_buffer(0);

    let started = player
        .start_streaming(preview_source(buffer.clone()), None, false, &FailingAudioDevice)
        .await;

    assert!(!started);
    assert!(player.active.is_none());
    assert!(buffer.is_cancelled());

    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, _capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);
    let next_buffer = create_sparse_buffer(0);

    let started = player.start_streaming(preview_source(next_buffer), None, false, &device).await;

    assert!(started);
    assert_eq!(player.current_target(), Some(&PreviewTarget::whole_file("preview.wav".into())));
    assert_eq!(player.stop(), AfterPreview::LeaveMain);
}

/// The preview listener maps a natural-completion audio event to a
/// `PreviewCompleted` command — the seam the service turns into teardown.
#[tokio::test]
async fn preview_listener_maps_completion_to_preview_completed() {
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = OwnCommands::channel();
    let player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());

    let (mut audio_tx, audio_rx) = audio_event_channel().unwrap();
    let handle = spawn_preview_listener(
        &player.diagnostics,
        player.progress_tx.clone(),
        player.command_tx.clone(),
        audio_rx,
    );

    audio_tx.push_required(AudioEvent::Completion((
        Arc::new(TrackFmt::for_test("preview.wav")),
        0,
        0,
    )));

    let cmd = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, command_rx.recv())
        .await
        .expect("the listener dispatches")
        .expect("a command is sent");
    assert!(matches!(cmd, PlaybackMessage::Own { command: PlaybackCommand::PreviewCompleted, .. }));
    handle.abort();
}

/// A preview seek whose stream rebuild fails tears the preview down outright
/// (no zombie) and surfaces `PreviewState::Idle` so the UI stops showing it.
#[tokio::test]
async fn failed_preview_seek_surfaces_idle() {
    let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());

    // Set up an active preview over a working capture output.
    let (captured, _capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);
    let buffer = create_sparse_buffer(0);
    assert!(player.start_streaming(preview_source(buffer), None, false, &device).await);
    assert!(player.is_active());

    // The retained output now fails to build a stream — a device that went
    // away mid-preview. Seeking must tear the preview down and notify the UI
    // rather than leaving a torn-down zombie. (The seek reuses the retained
    // output, so the device passed here is never opened from.)
    player.audio_output = Some(crate::audio_output::PlayerOutput::new(
        Box::new(FailingAudioOutput),
        crate::audio_output::AudioOutputControls::new(crate::Volume::FULL),
    ));
    player.seek(crate::StreamPosition::from_millis(500), &device).await;

    assert!(!player.is_active(), "a failed seek leaves no active preview");
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PreviewStateChanged(PreviewState::Idle)) {
            saw_idle = true;
        }
    }
    assert!(saw_idle, "a failed preview seek surfaces PreviewState::Idle to the UI");
}

/// Neither shape of "no usable probe" — the prober ran and found nothing
/// (`Ok(None)`, a file that isn't audio) nor the probe task itself failed
/// (`Err`, a panic) — resolves to a probe; both fall through to the same
/// `None` rather than a caller mistaking either for a usable format and
/// driving a decoder with defaults.
#[tokio::test]
async fn resolve_preview_probe_rejects_absent_or_failed_probe() {
    let unprobeable: Result<Option<bae_audio::ProbeResult>, TaskFailed> = Ok(None);
    assert!(resolve_preview_probe(std::path::Path::new("path"), unprobeable).is_none());

    let probe_panicked = Err(TaskFailed::Panicked(bae_util::task_failure::Bug {
        operation: "probe the file to preview",
        message: "preview probe panic".to_string(),
    }));
    assert!(resolve_preview_probe(std::path::Path::new("path"), probe_panicked).is_none());
}

fn probe(sample_rate: u32, channels: u32) -> bae_audio::ProbeResult {
    let count = |count| std::num::NonZeroU32::new(count).expect("a test count");
    bae_audio::ProbeResult {
        content_type: bae_util::content_type::ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
        duration: Duration::from_secs(1),
        sample_rate: count(sample_rate),
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels: count(channels),
    }
}

/// A probe resolves to itself.
#[test]
fn resolve_preview_probe_accepts_usable_format() {
    let result: Result<_, TaskFailed> = Ok(Some(probe(44100, 2)));
    let resolved = resolve_preview_probe(std::path::Path::new("path"), result)
        .expect("a usable format passes");
    assert_eq!(resolved.sample_rate.get(), 44100);
    assert_eq!(resolved.channels.get(), 2);
}

/// A preview holds its file open while it plays; stopping it, or switching
/// to another file, closes it.
#[tokio::test(flavor = "multi_thread")]
async fn ending_a_preview_closes_its_file() {
    bae_audio::init();
    let dir = tempfile::TempDir::new().unwrap();
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
    let first = dir.path().join("first.flac");
    let second = dir.path().join("second.flac");
    std::fs::copy(fixture.join("Test Album.flac"), &first).unwrap();
    std::fs::copy(fixture.join("Test Album.flac"), &second).unwrap();
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = OwnCommands::channel();
    let mut player =
        PreviewPlayer::new(progress_tx, command_tx, 50, bae_diagnostics::Diagnostics::noop());
    let (captured, _capture_rx) = tokio_mpsc::unbounded_channel();
    let device = CaptureAudioDevice::new(captured);

    let open_now = |path: &std::path::Path| {
        let path = path.canonicalize().unwrap();
        coven::open_files_under(dir.path()).into_iter().filter(|open| *open == path).count()
    };
    // The preview's fill opens its file on its own task, holds it until the
    // preview ends, and closes it when it ends.
    let settled = |player: &PreviewPlayer| {
        player.active.as_ref().expect("a preview is loaded").source.buffer.clone()
    };
    let ended = |player: &mut PreviewPlayer| {
        let fill = player.fills.remove(0);
        async move { fill.ended().await }
    };

    let whole = |path: &std::path::Path| PreviewTarget::whole_file(path.to_path_buf());
    assert!(player.play(whole(&first), &device).await.started());
    settled(&player).wait_until_fill_settled().await;
    assert_eq!(open_now(&first), 1);

    assert!(player.play(whole(&second), &device).await.started());
    ended(&mut player).await;
    assert_eq!(open_now(&first), 0);
    settled(&player).wait_until_fill_settled().await;
    assert_eq!(open_now(&second), 1);

    assert_eq!(player.stop(), AfterPreview::LeaveMain);
    ended(&mut player).await;
    assert_eq!(open_now(&second), 0);
    coven::assert_no_open_files_under(dir.path());
}
