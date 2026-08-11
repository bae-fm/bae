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
fn pts_to_sample_fast_paths_when_time_base_is_sample_rate() {
    // time_base == 1/sample_rate: the pts already counts samples, returned as-is.
    assert_eq!(pts_to_sample(1000, rational(1, 44_100), 44_100), 1000);
    // Millisecond time base scales up: 500ms * 44100/1000 = 22050 samples.
    assert_eq!(pts_to_sample(500, rational(1, 1000), 44_100), 22_050);
    // CUE-frame time base (1/75) at 75 frames -> exactly one second of samples.
    assert_eq!(pts_to_sample(75, rational(1, 75), 44_100), 44_100);
}

#[test]
fn frame_sample_bounds_tracks_position_across_pts_gaps() {
    let tb = rational(1, 44_100);

    // A valid pts seeds the tracked position and advances it to the frame end.
    let mut tracked = -1;
    assert_eq!(
        frame_sample_bounds(1000, 100, tb, 44_100, &mut tracked),
        (1000, 1100)
    );
    assert_eq!(tracked, 1100);

    // A frame with no pts continues from the tracked position.
    assert_eq!(
        frame_sample_bounds(
            ffmpeg_sys_next::AV_NOPTS_VALUE,
            200,
            tb,
            44_100,
            &mut tracked
        ),
        (1100, 1300)
    );
    assert_eq!(tracked, 1300);

    // No pts and no tracked position yet: start is unknown (-1), and the
    // tracked position stays unset rather than advancing from -1.
    let mut untracked = -1;
    assert_eq!(
        frame_sample_bounds(
            ffmpeg_sys_next::AV_NOPTS_VALUE,
            50,
            tb,
            44_100,
            &mut untracked
        ),
        (-1, 49)
    );
    assert_eq!(untracked, -1);
}

#[test]
fn frame_output_window_trims_at_boundaries() {
    use FrameOutputWindow::*;

    struct Row {
        name: &'static str,
        frame_start: i64,
        frame_end: i64,
        output_len: usize,
        channels: usize,
        start: Option<u64>,
        end: Option<u64>,
        expected: FrameOutputWindow,
    }
    let rows = [
        Row {
            name: "no window emits whole frame",
            frame_start: 0,
            frame_end: 100,
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
            frame_start: 0,
            frame_end: 0,
            output_len: 0,
            channels: 2,
            start: None,
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame entirely before start skips",
            frame_start: 0,
            frame_end: 100,
            output_len: 200,
            channels: 2,
            start: Some(200),
            end: None,
            expected: Skip,
        },
        Row {
            name: "start at frame_end is exclusive, skips",
            frame_start: 0,
            frame_end: 100,
            output_len: 200,
            channels: 2,
            start: Some(100),
            end: None,
            expected: Skip,
        },
        Row {
            name: "frame straddling start trims the lead-in",
            frame_start: 0,
            frame_end: 100,
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
            frame_start: 200,
            frame_end: 300,
            output_len: 200,
            channels: 2,
            start: None,
            end: Some(200),
            expected: Stop,
        },
        Row {
            name: "frame straddling end trims the tail and reaches end",
            frame_start: 0,
            frame_end: 100,
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
            frame_start: 0,
            frame_end: 100,
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
            frame_start: 0,
            frame_end: 100,
            output_len: 100,
            channels: 1,
            start: Some(70),
            end: Some(60),
            expected: Stop,
        },
        Row {
            name: "start-skip clamped to output with no end skips",
            frame_start: 0,
            frame_end: 100,
            output_len: 50,
            channels: 1,
            start: Some(80),
            end: None,
            expected: Skip,
        },
        Row {
            name: "unknown frame_start disables trimming",
            frame_start: -1,
            frame_end: 99,
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
        let got = frame_output_window(
            row.frame_start,
            row.frame_end,
            row.output_len,
            row.channels,
            row.start,
            row.end,
        );
        assert_eq!(got, row.expected, "{}", row.name);
    }
}

#[test]
fn packed_s64_decoder_rejects_zero_channels_before_format_signal() {
    crate::audio_codec::init();

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
