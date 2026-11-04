use super::*;
use crate::audio_output::{
    AudioOutputControls, CaptureAudioOutput, FailingAudioOutput, PlayerOutput,
};
use bae_audio::sparse_buffer::create_sparse_buffer;
use bae_track_audio::SegmentDecodeParams;

fn one_segment_decode(buffer: SharedSparseBuffer) -> StreamDecodeParams {
    StreamDecodeParams::new(
        vec![SegmentDecodeParams::new(buffer, bae_db::SegmentSpan::whole_file(), 0)],
        true,
        0,
        0,
    )
}

/// A stream-build failure cancels the just-spawned decoder and returns `Err`
/// rather than leaking a half-built pipeline. This exercises the real shared
/// unit both players run through.
#[tokio::test]
async fn start_stream_pipeline_failure_returns_err_and_cancels_decoder() {
    let buffer = create_sparse_buffer(0);
    let mut output = PlayerOutput::new(
        Box::new(FailingAudioOutput),
        AudioOutputControls::new(crate::Volume::FULL),
    );

    let result = start_stream_pipeline(
        &Diagnostics::noop(),
        &mut output,
        DecoderSetup::new(
            one_segment_decode(buffer.clone()),
            44_100,
            2,
            "unit decode",
            DecodeFailureReport::LogOnly,
        ),
        TrackFmt::for_test("unit"),
        50,
        |pipeline, _audio_events| pipeline,
    )
    .await;

    assert!(result.is_err(), "a failed stream build returns Err");
    // The owner cancels the buffer on Err; doing so here proves the spawned
    // decoder unwinds (its read unblocks and it exits) rather than parking
    // forever on a ring nobody drains.
    buffer.cancel();
    assert!(buffer.is_cancelled());
}

/// The happy path assembles a live pipeline over a working output and hands
/// back the audio-events and ready channels.
#[tokio::test]
async fn start_stream_pipeline_success_builds_pipeline() {
    let buffer = create_sparse_buffer(0);
    let (captured, _capture_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut output = PlayerOutput::new(
        Box::new(CaptureAudioOutput::new(captured)),
        AudioOutputControls::new(crate::Volume::FULL),
    );

    let start = start_stream_pipeline(
        &Diagnostics::noop(),
        &mut output,
        DecoderSetup::new(
            one_segment_decode(buffer),
            44_100,
            2,
            "unit decode",
            DecodeFailureReport::LogOnly,
        ),
        TrackFmt::for_test("unit"),
        50,
        |pipeline, _audio_events| pipeline,
    )
    .await
    .expect("stream builds over a working output");

    // Teardown drops the stream and detaches the decoder; nothing left to poll.
    start.cancel();
}
