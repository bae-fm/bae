/// An output whose volume read panics: a bug in one command's handling.
struct VolumePanicsOutput(AudioOutputControls);

impl crate::playback::audio_output::AudioOutput for VolumePanicsOutput {
    fn create_stream(
        &mut self,
        _source: Arc<Mutex<source::PlaybackSource>>,
        _source_sample_rate: u32,
        _source_channels: u32,
        _audio_events: AudioEventSender,
        _position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        Err(AudioError::Unexpected("this test's output builds no stream".to_string()))
    }

    fn controls(&self) -> &AudioOutputControls {
        &self.0
    }

    fn get_volume(&self) -> f32 {
        panic!("volume cell missing")
    }
}

/// A command that panics is reported, stops playback with an error, and the
/// service goes on to serve the next command. Sabotage: drop the boundary in
/// `run`, and the panic ends the service, so the next command is never
/// answered.
#[tokio::test]
async fn a_command_that_panics_is_reported_and_the_next_is_served() {
    let bugs = Arc::new(crate::diagnostics::RecordingBugReporter::default());
    let diagnostics = crate::diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let store = seeded_store(&[]).await;
    let (mut service, mut progress_rx) = playback_service_over(&store, diagnostics.clone());
    service.audio_output = Box::new(VolumePanicsOutput(AudioOutputControls::new(1.0)));
    let command_tx = service.command_tx.clone();
    let (_deletions_tx, deletions_rx) = tokio_mpsc::unbounded_channel();
    let (volume_tx, volume_rx) = oneshot::channel();
    let (saved_tx, saved_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    dispatch_command(&command_tx, PlaybackCommand::GetVolume(volume_tx));
    dispatch_command(&command_tx, PlaybackCommand::SaveState(saved_tx));
    dispatch_command(&command_tx, PlaybackCommand::Shutdown(shutdown_tx));

    service.run(deletions_rx).await;

    assert!(volume_rx.await.is_err(), "the command that panicked answers nothing");
    saved_rx.await.expect("the next command is served");
    shutdown_rx.await.expect("the service shuts down when asked");
    let mut saw_error = false;
    while let Ok(progress) = progress_rx.try_recv() {
        saw_error |= matches!(progress, PlaybackProgress::PlaybackError { .. });
    }
    assert!(saw_error, "the person is told playback stopped");
    assert_eq!(
        bugs.reports(),
        vec![(
            "serve playback".to_string(),
            "serve playback panicked: volume cell missing".to_string()
        )]
    );
}
