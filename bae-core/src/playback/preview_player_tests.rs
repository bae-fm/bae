use super::*;
use crate::playback::audio_output::{
    audio_event_channel, CaptureAudioDevice, FailingAudioDevice, FailingAudioOutput,
};
use std::sync::Arc;

#[tokio::test]
async fn preview_play_rejects_unprobeable_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("not-audio.bin");
    std::fs::write(&path, b"not audio").unwrap();
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let (device, _capture_rx) = CaptureAudioDevice::new();

    let started = player
        .play(
            PreviewTarget::whole_file(path.display().to_string()),
            &device,
        )
        .await
        .started();
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
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let (device, _capture_rx) = CaptureAudioDevice::new();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cue_flac/Test Album.flac")
        .display()
        .to_string();
    let sample_rate = u64::from(
        crate::audio_codec::probe_audio_from_path(&path)
            .expect("probe fixture")
            .sample_rate,
    );

    let first = PreviewTarget::sample_range(path.clone(), 0, Some(sample_rate));
    assert!(player.play(first, &device).await.started());
    player.main_player_paused();

    let second = PreviewTarget::sample_range(path, 10 * sample_rate, Some(11 * sample_rate));
    assert!(player.play(second, &device).await.started());

    assert_eq!(player.stop(), AfterPreview::ResumeMain);
    assert_eq!(
        player.stop(),
        AfterPreview::LeaveMain,
        "the resume is handed back once"
    );
}

#[tokio::test]
async fn preview_plays_only_the_requested_sample_window() {
    let fixture_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let path = fixture_dir.join("Test Album.flac");
    let probe =
        crate::audio_codec::probe_audio_from_path(path.to_str().expect("fixture path is UTF-8"))
            .expect("probe fixture");
    let sample_rate = u64::from(probe.sample_rate);
    let target = PreviewTarget::sample_range(
        path.display().to_string(),
        10 * sample_rate,
        Some(11 * sample_rate),
    );

    let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let (device, mut capture_rx) = CaptureAudioDevice::new();

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

    let captured = capture_rx.recv().await.expect("preview capture");
    let completed = tokio::time::timeout(Duration::from_secs(10), command_rx.recv())
        .await
        .expect("bounded preview completes within the timeout")
        .expect("bounded preview sends its completion command");
    assert!(matches!(completed, PlaybackCommand::PreviewCompleted));
    let captured = captured.lock().expect("preview capture lock").clone();
    assert_eq!(
        captured.len(),
        probe.sample_rate as usize * probe.channels as usize,
        "a one-second source window emits exactly one second of samples"
    );
    let reference_path = fixture_dir.join("02 Test Artist - Track Two (White Noise).flac");
    let reference_bytes = std::fs::read(reference_path).expect("read reference fixture");
    let reference_buffer = create_sparse_buffer(reference_bytes.len() as u64);
    reference_buffer.append_at(0, &reference_bytes);
    let reference = crate::audio_codec::decode_audio(reference_buffer, None, None)
        .expect("decode reference fixture");
    let expected: Vec<f32> = reference
        .samples
        .iter()
        .take(1_000)
        .map(|sample| *sample as f32 / i32::MAX as f32)
        .collect();
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
    let fixture_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let path = fixture_dir.join("Test Album.flac");
    let probe =
        crate::audio_codec::probe_audio_from_path(path.to_str().expect("fixture path is UTF-8"))
            .expect("probe fixture");
    let sample_rate = u64::from(probe.sample_rate);
    let target = PreviewTarget::sample_range(
        path.display().to_string(),
        10 * sample_rate,
        Some(11 * sample_rate),
    );
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let (device, mut capture_rx) = crate::playback::audio_output::RealtimeCaptureAudioDevice::new();

    assert!(player.play(target, &device).await.started());
    player.toggle_pause();
    let _initial_capture = capture_rx.recv().await.expect("initial preview capture");

    player.seek_by_ratio(0.5, &device).await;
    let seeked_capture = capture_rx.recv().await.expect("seeked preview capture");
    player.toggle_pause();
    let captured = crate::playback::audio_output::wait_for_samples(
        &seeked_capture,
        1_000,
        Duration::from_secs(10),
    )
    .await;

    let reference_path = fixture_dir.join("02 Test Artist - Track Two (White Noise).flac");
    let reference_bytes = std::fs::read(reference_path).expect("read reference fixture");
    let reference_buffer = create_sparse_buffer(reference_bytes.len() as u64);
    reference_buffer.append_at(0, &reference_bytes);
    let reference = crate::audio_codec::decode_audio(reference_buffer, None, None)
        .expect("decode reference fixture");
    let half_second = probe.sample_rate as usize * probe.channels as usize / 2;
    let expected: Vec<f32> = reference
        .samples
        .iter()
        .skip(half_second)
        .take(1_000)
        .map(|sample| *sample as f32 / i32::MAX as f32)
        .collect();
    assert!(captured.len() >= expected.len());
    for (index, (actual, expected)) in captured.iter().zip(&expected).enumerate() {
        assert!(
            (actual - expected).abs() < 0.01,
            "seeked preview sample {index} differs: {actual} vs {expected}"
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
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let buffer = create_sparse_buffer(0);

    let started = player
        .start_streaming(
            PreviewTarget::whole_file("preview.wav".to_string()),
            Duration::from_secs(1),
            44_100,
            2,
            buffer.clone(),
            None,
            false,
            &FailingAudioDevice,
        )
        .await;

    assert!(!started);
    assert!(player.active.is_none());
    assert!(buffer.is_cancelled());

    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
    let (device, _capture_rx) = CaptureAudioDevice::new();
    let next_buffer = create_sparse_buffer(0);

    let started = player
        .start_streaming(
            PreviewTarget::whole_file("preview.wav".to_string()),
            Duration::from_secs(1),
            44_100,
            2,
            next_buffer,
            None,
            false,
            &device,
        )
        .await;

    assert!(started);
    assert_eq!(
        player.current_target(),
        Some(&PreviewTarget::whole_file("preview.wav".to_string()))
    );
    assert_eq!(player.stop(), AfterPreview::LeaveMain);
}

/// The preview listener maps a natural-completion audio event to a
/// `PreviewCompleted` command — the seam the service turns into teardown.
#[tokio::test]
async fn preview_listener_maps_completion_to_preview_completed() {
    let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = tokio_mpsc::unbounded_channel();
    let player = PreviewPlayer::new(progress_tx, command_tx, 50);

    let (mut audio_tx, audio_rx) = audio_event_channel();
    let handle = spawn_preview_listener(
        player.progress_tx.clone(),
        player.command_tx.clone(),
        audio_rx,
    );

    audio_tx.push_required(AudioEvent::Completion((
        Arc::new(TrackFmt {
            track_id: "preview.wav".to_string(),
            duration_ms: 1_000,
            pregap_ms: None,
            position_offset: Duration::ZERO,
            replay_gain_linear: 1.0,
        }),
        0,
        0,
    )));

    let cmd = tokio::time::timeout(Duration::from_secs(1), command_rx.recv())
        .await
        .expect("the listener dispatches within the timeout")
        .expect("a command is sent");
    assert!(matches!(cmd, PlaybackCommand::PreviewCompleted));
    handle.abort();
}

/// A preview seek whose stream rebuild fails tears the preview down outright
/// (no zombie) and surfaces `PreviewState::Idle` so the UI stops showing it.
#[tokio::test]
async fn failed_preview_seek_surfaces_idle() {
    let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
    let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
    let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);

    // Set up an active preview over a working capture output.
    let (device, _capture_rx) = CaptureAudioDevice::new();
    let buffer = create_sparse_buffer(0);
    assert!(
        player
            .start_streaming(
                PreviewTarget::whole_file("preview.wav".to_string()),
                Duration::from_secs(1),
                44_100,
                2,
                buffer,
                None,
                false,
                &device,
            )
            .await
    );
    assert!(player.is_active());

    // The retained output now fails to build a stream — a device that went
    // away mid-preview. Seeking must tear the preview down and notify the UI
    // rather than leaving a torn-down zombie. (The seek reuses the retained
    // output, so the device passed here is never opened from.)
    player.audio_output = Some(Box::new(FailingAudioOutput));
    player.seek(Duration::from_millis(500), &device).await;

    assert!(
        !player.is_active(),
        "a failed seek leaves no active preview"
    );
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(
            progress,
            PlaybackProgress::PreviewStateChanged(PreviewState::Idle)
        ) {
            saw_idle = true;
        }
    }
    assert!(
        saw_idle,
        "a failed preview seek surfaces PreviewState::Idle to the UI"
    );
}

/// Neither shape of "no usable probe" — the prober ran and found nothing
/// (`Ok(None)`, a file that isn't audio) nor the probe task itself failed
/// (`Err`, a panic) — resolves to a probe; both fall through to the same
/// `None` rather than a caller mistaking either for a usable format and
/// driving a decoder with defaults.
#[tokio::test]
async fn resolve_preview_probe_rejects_absent_or_failed_probe() {
    let unprobeable: Result<Option<crate::audio_codec::ProbeResult>, tokio::task::JoinError> =
        Ok(None);
    assert!(resolve_preview_probe("path", unprobeable).is_none());

    let join_failed = tokio::spawn(async { panic!("preview probe panic") })
        .await
        .map(|()| Option::<crate::audio_codec::ProbeResult>::None);
    assert!(resolve_preview_probe("path", join_failed).is_none());
}

fn probe(sample_rate: u32, channels: u32) -> crate::audio_codec::ProbeResult {
    crate::audio_codec::ProbeResult {
        content_type: crate::util::content_type::ContentType::Flac,
        duration: Duration::from_secs(1),
        sample_rate,
        bits_per_sample: Some(16),
        bitrate_kbps: None,
        channels,
    }
}

/// A usable probe (positive sample rate and channels) resolves to the probe.
#[test]
fn resolve_preview_probe_accepts_usable_format() {
    let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(44100, 2)));
    let resolved = resolve_preview_probe("path", result).expect("a usable format passes");
    assert_eq!(resolved.sample_rate, 44100);
    assert_eq!(resolved.channels, 2);
}

/// A probe reporting a zero sample rate is an unusable format and is
/// rejected rather than driving a decoder with a nonsense rate.
#[test]
fn resolve_preview_probe_rejects_zero_sample_rate() {
    let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(0, 2)));
    assert!(resolve_preview_probe("path", result).is_none());
}

/// A probe reporting zero channels is likewise unusable.
#[test]
fn resolve_preview_probe_rejects_zero_channels() {
    let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(44100, 0)));
    assert!(resolve_preview_probe("path", result).is_none());
}
