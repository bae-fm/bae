use super::*;

#[derive(Default)]
struct TestDecodedSink {
    format: Option<(u32, u32)>,
}

impl DecodedSink for TestDecodedSink {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        self.format = Some((sample_rate, channels));
    }

    fn on_samples(&mut self, _samples: &[i32]) {}
}

fn rational(num: c_int, den: c_int) -> ffmpeg_sys_next::AVRational {
    ffmpeg_sys_next::AVRational { num, den }
}

#[test]
fn a_sample_clock_counts_samples_from_the_streams_start() {
    // A time base of one sample: timestamps count samples.
    let samples = SampleClock::new(rational(1, 44_100), 44_100, 0);
    assert_eq!(samples.sample_at(1000), 1000);
    assert_eq!(samples.timestamp_of(1000), 1000);
    // A millisecond time base: 500 ms is 22,050 samples, and back.
    let millis = SampleClock::new(rational(1, 1000), 44_100, 0);
    assert_eq!(millis.sample_at(500), 22_050);
    assert_eq!(millis.timestamp_of(22_050), 500);
    // An MP3's: 1/14,112,000 s, its first sample after a 1,105-sample delay.
    let mp3 = SampleClock::new(rational(1, 14_112_000), 44_100, 1_105 * 320);
    assert_eq!(mp3.sample_at(1_105 * 320), 0);
    assert_eq!(mp3.sample_at(0), -1_105);
    assert_eq!(mp3.timestamp_of(4_000), (1_105 + 4_000) * 320);
    assert_eq!(mp3.timestamp_of(-1_105), 0);
}

#[test]
fn frame_sample_bounds_tracks_position_across_pts_gaps() {
    let clock = SampleClock::new(rational(1, 44_100), 44_100, 0);

    // A valid pts places the frame and advances the tracked position to its end.
    let mut tracked = None;
    assert_eq!(
        frame_sample_bounds(1000, 100, &clock, &mut tracked),
        Some((1000, 1100))
    );
    assert_eq!(tracked, Some(1100));

    // A frame with no pts continues from the tracked position.
    assert_eq!(
        frame_sample_bounds(ffmpeg_sys_next::AV_NOPTS_VALUE, 200, &clock, &mut tracked),
        Some((1100, 1300))
    );
    assert_eq!(tracked, Some(1300));

    // No pts and no tracked position yet: the frame is unplaced, and so is
    // the next.
    let mut untracked = None;
    assert_eq!(
        frame_sample_bounds(ffmpeg_sys_next::AV_NOPTS_VALUE, 50, &clock, &mut untracked),
        None
    );
    assert_eq!(untracked, None);

    // A frame before the stream's first sample (an encoder delay the decoder
    // kept) is placed before it.
    let mut before = None;
    assert_eq!(
        frame_sample_bounds(-312, 960, &clock, &mut before),
        Some((-312, 648))
    );
}

#[test]
fn frame_output_window_trims_at_boundaries() {
    use FrameOutputWindow::*;

    struct Row {
        name: &'static str,
        bounds: Option<(i64, i64)>,
        output_len: usize,
        channels: usize,
        start: Option<u64>,
        end: Option<u64>,
        expected: FrameOutputWindow,
    }
    let rows = [
        Row {
            name: "no window emits whole frame",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: None,
            end: None,
            expected: Emit {
                skip_start: 0,
                take_end: 200,
                reached_end: false,
            },
        },
        Row {
            name: "empty output with no window skips",
            bounds: Some((0, 0)),
            output_len: 0,
            channels: 2,
            start: None,
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame entirely before start skips",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(200),
            end: None,
            expected: Skip,
        },
        Row {
            name: "start at frame_end is exclusive, skips",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(100),
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame straddling start trims the lead-in",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: Some(40),
            end: None,
            expected: Emit {
                skip_start: 80,
                take_end: 200,
                reached_end: false,
            },
        },
        Row {
            name: "frame at or past end stops",
            bounds: Some((200, 300)),
            output_len: 200,
            channels: 2,
            start: None,
            end: Some(200),
            expected: Stop,
        },
        Row {
            name: "frame straddling end trims the tail and reaches end",
            bounds: Some((0, 100)),
            output_len: 200,
            channels: 2,
            start: None,
            end: Some(60),
            expected: Emit {
                skip_start: 0,
                take_end: 120,
                reached_end: true,
            },
        },
        Row {
            name: "take_end clamps to a short output buffer",
            bounds: Some((0, 100)),
            output_len: 50,
            channels: 1,
            start: None,
            end: Some(80),
            expected: Emit {
                skip_start: 0,
                take_end: 50,
                reached_end: true,
            },
        },
        Row {
            name: "start-skip past a reached end stops",
            bounds: Some((0, 100)),
            output_len: 100,
            channels: 1,
            start: Some(70),
            end: Some(60),
            expected: Stop,
        },
        Row {
            name: "start-skip clamped to output with no end skips",
            bounds: Some((0, 100)),
            output_len: 50,
            channels: 1,
            start: Some(80),
            end: None,
            expected: Skip,
        },
        Row {
            name: "an unplaced frame disables trimming",
            bounds: None,
            output_len: 200,
            channels: 2,
            start: Some(50),
            end: Some(60),
            expected: Emit {
                skip_start: 0,
                take_end: 200,
                reached_end: false,
            },
        },
    ];

    for row in rows {
        let got = frame_output_window(row.bounds, row.output_len, row.channels, row.start, row.end);
        assert_eq!(got, row.expected, "{}", row.name);
    }
}

#[test]
fn packed_s64_decoder_rejects_zero_channels_before_format_signal() {
    crate::init();

    unsafe {
        let fmt_ctx = ffmpeg_sys_next::avformat_alloc_context();
        assert!(!fmt_ctx.is_null());
        let stream = ffmpeg_sys_next::avformat_new_stream(fmt_ctx, ptr::null());
        assert!(!stream.is_null());
        (*stream).time_base = ffmpeg_sys_next::AVRational {
            num: 1,
            den: 44_100,
        };
        let mut codecpar = ffmpeg_sys_next::avcodec_parameters_alloc();
        assert!(!codecpar.is_null());
        (*codecpar).ch_layout.nb_channels = 0;

        let mut sink = TestDecodedSink::default();
        let result = decode_packed_s64_packets_to_sink(
            fmt_ctx, 0, stream, codecpar, 44_100, 0, None, None, &mut sink,
        );

        ffmpeg_sys_next::avcodec_parameters_free(&mut codecpar);
        ffmpeg_sys_next::avformat_free_context(fmt_ctx);

        let err = result.expect_err("zero-channel PCM should fail");
        assert!(
            err.contains("channel count must be greater than zero"),
            "unexpected decode error: {err}"
        );
        assert_eq!(sink.format, None);
    }
}
