use super::TrackDecode;
use std::sync::Arc;

/// A sink that keeps nothing: the decode under test fails before any audio.
struct DiscardingSink;

impl bae_audio::DecodedSink for DiscardingSink {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
}

/// A read of a track decode's source that panics, under a transcode, fails
/// the decode with the read's bug, which the decode doesn't report again: the
/// panic hook reported it.
#[test]
fn a_track_decode_whose_read_panics_fails_with_its_bug() {
    bae_audio::init();
    let bytes = std::fs::read(
        bae_audio::test_fixtures::cue_flac_dir()
            .join("03 Test Artist - Track Three (Brown Noise).flac"),
    )
    .expect("fixture");
    let buffer = bae_audio::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, &bytes);
    buffer.panic_reads();
    let reports = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let decode = TrackDecode::new(
        bae_track_audio::StreamDecodeParams::new(
            vec![bae_track_audio::SegmentDecodeParams::new(
                buffer,
                bae_records::SegmentSpan::whole_file(),
                0,
            )],
            true,
            0,
            0,
        ),
        44_100,
        2,
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(reports.clone()),
    );

    let result = decode
        .run_to_sink(&mut DiscardingSink, Arc::new(std::sync::atomic::AtomicBool::new(false)));

    let error = result.expect_err("a read that panicked fails the decode");
    assert!(error.contains(" panicked: "), "{error}");
    assert_eq!(reports.reports(), Vec::new());
}
