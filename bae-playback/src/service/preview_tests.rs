use super::*;
use crate::audio_output::AudioStream;
use crate::service::tests::*;
use crate::stream_pipeline::StreamPipeline;
use bae_audio::track_stream::create_track_stream_pair;

/// Build a `StreamPipeline` over `stream` and a fresh source for the prepared
/// track's fmt — the per-track unit the preview player still holds. The stub
/// decoder/token come from `new_for_test`.
fn test_pipeline(prepared: &PlaybackPreparedTrack, stream: Box<dyn AudioStream>) -> StreamPipeline {
    let (_sink, source, _ready) = create_track_stream_pair(prepared.sample_rate, prepared.channels);
    let track_fmt = prepared.track_fmt(StreamPosition::START);
    StreamPipeline::new_for_test(
        Arc::new(Mutex::new(source::PlaybackSource::new(source, track_fmt))),
        stream,
    )
}

/// A preview playing to its end tears the preview down and emits
/// `PreviewState::Idle`.
#[tokio::test]
async fn preview_completed_tears_down_and_emits_idle() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline = test_pipeline(&prepared, Box::new(TestAudioStream));
    service.preview.set_active_for_test("preview-file".into(), pipeline, buffer.clone());
    assert!(service.preview.is_active());

    service.preview_completed();

    assert!(!service.preview.is_active(), "completion tears the active preview down");
    assert!(buffer.is_cancelled(), "completion cancels the preview buffer");
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PreviewStateChanged(crate::PreviewState::Idle)) {
            saw_idle = true;
        }
    }
    assert!(saw_idle, "completion emits PreviewState::Idle");
}

/// A preview whose stream's device reported it can't go on, and which can't
/// start again because there is no device to start on, ends: the preview is
/// torn down, the UI is told it is idle, and the main player gets back the
/// resume the preview owed it. Sabotage: skip the `preview_stop` after the
/// failed restart, and the main player stays paused.
#[tokio::test]
async fn an_invalidated_preview_that_cant_restart_ends_and_resumes_main() {
    let (_home, mut service, mut progress_rx) = test_playback_service().await;
    let main_buffer = create_sparse_buffer(1_024);
    service.slot =
        active_slot(test_prepared_track("t", main_buffer), TrackPhase::Paused(PausePhase::Manual));

    let buffer = create_sparse_buffer(1_024);
    let prepared = test_prepared_track("preview-file", buffer.clone());
    let pipeline =
        test_pipeline(&prepared, Box::new(InvalidatedTestStream::new("device disconnected")));
    service.preview.set_active_for_test("preview-file".into(), pipeline, buffer.clone());
    service.preview.main_player_paused();

    service.follow_preview_output_device().await;

    assert!(!service.preview.is_active(), "the preview ended");
    assert!(buffer.is_cancelled(), "its buffer is cancelled");
    assert!(
        matches!(&service.slot, PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing)),
        "the main player got its resume back"
    );
    let mut saw_idle = false;
    while let Ok(progress) = progress_rx.try_recv() {
        if matches!(progress, PlaybackProgress::PreviewStateChanged(crate::PreviewState::Idle)) {
            saw_idle = true;
        }
    }
    assert!(saw_idle, "the UI is told the preview is idle");
}
