use super::*;
use bae_audio::test_samples::filled_sparse_buffer;

/// Counts the frames a decode hands it.
#[derive(Default)]
struct FrameCount {
    channels: u32,
    frames: u64,
}

impl bae_audio::DecodedSink for FrameCount {
    fn on_format(&mut self, _sample_rate: u32, channels: u32) {
        self.channels = channels;
    }

    fn on_samples(&mut self, samples: &[i32]) {
        self.frames += (samples.len() / self.channels.max(1) as usize) as u64;
    }
}

/// The byte offsets a verifying decode of one second from 20s into the
/// CUE fixture image read, with the window's recorded landing byte, and
/// the frames it decoded.
fn verified_window_reads() -> (Vec<u64>, u64, u64) {
    bae_audio::init();
    let path = bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac");
    let bytes = std::fs::read(&path).unwrap();
    let start_sample = 20 * 44_100;
    let landing = bae_audio::seek_landing_bytes(&path, &[start_sample]).expect("landing byte")[0];
    let buffer = filled_sparse_buffer(&bytes);
    let decode = StreamDecodeParams::new(
        vec![SegmentDecodeParams::new(
            buffer.clone(),
            bae_db::SegmentSpan {
                start_sample,
                end_sample: Some(start_sample + 44_100),
                start_byte: Some(landing),
                end_byte: None,
            },
            0,
        )],
        true,
        0,
        0,
    );
    let mut sink = FrameCount::default();
    decode.verify_to_sink(&mut sink, Arc::new(AtomicBool::new(false))).expect("the window decodes");
    (buffer.read_log(), landing, sink.frames)
}

/// A verifying decode jumps straight to a segment's recorded landing byte
/// rather than searching the file for its start sample: past the header,
/// it reads nothing before the landing.
#[test]
fn verifying_decode_jumps_to_the_recorded_landing_byte() {
    let (reads, landing, frames) = verified_window_reads();
    assert_eq!(frames, 44_100);
    let header_end = reads[0] + 8 * 1024;
    assert!(
        reads.iter().all(|&offset| offset < header_end || offset >= landing),
        "reads {reads:?} around landing {landing}"
    );

    assert!(reads.contains(&landing), "the decode reads from the landing: {reads:?} vs {landing}");
}

/// A track made of several segments (a CUE pregap segment followed by its
/// main segment) finishes once, after the last segment. Finishing the sink
/// after the first segment makes the player treat the pregap as the whole
/// track and advance a few seconds in.
#[test]
fn multi_segment_track_finishes_only_after_last_segment() {
    use bae_audio::track_stream::create_track_stream_pair_with_capacity;
    use bae_audio::{encode_i32, EncodeFormat};

    let sample_rate = 44_100u32;
    let total_frames = 2 * sample_rate as usize;
    let original: Vec<i32> = (0..total_frames)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
        })
        .collect();
    let flac =
        encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &original, sample_rate, 1).unwrap();
    let buffer = filled_sparse_buffer(&flac);

    let split = sample_rate as u64;
    let span = |start: u64, end: u64| bae_db::SegmentSpan {
        start_sample: start,
        end_sample: Some(end),
        start_byte: None,
        end_byte: None,
    };
    let decode = StreamDecodeParams::new(
        vec![
            SegmentDecodeParams::new(buffer.clone(), span(0, split), 0),
            SegmentDecodeParams::new(buffer, span(split, total_frames as u64), 0),
        ],
        false,
        0,
        0,
    );

    // A ring far smaller than a segment, so the decoder is pushing
    // samples -- not finished -- for most of the run, and a sink finished
    // at the segment boundary is observed before the second segment's
    // samples arrive.
    let (mut sink, mut stream, _ready) =
        create_track_stream_pair_with_capacity(sample_rate, 1, 4_096);
    let decoder =
        std::thread::spawn(move || decode.run_decoder(&mut sink, Arc::new(AtomicBool::new(false))));

    let mut pulled = 0usize;
    let mut buf = [0.0f32; 1_024];
    loop {
        pulled += stream.pull_samples(&mut buf);
        if stream.producer_finished() {
            assert_eq!(
                stream.samples_decoded() as usize,
                total_frames,
                "sink reported finished before every segment was decoded"
            );
            if stream.is_finished() {
                break;
            }
        }
    }
    decoder.join().unwrap().expect("two-segment decode succeeds");
    assert_eq!(pulled, total_frames);
}

/// The byte a fresh buffer's reader fetches first is where its window's
/// decode first reads: the landing byte at a natural start, the scrub point
/// a position starts from, and nothing it can know for a sample seek inside
/// the segment or a decode from the stream's start.
#[test]
fn a_window_first_reads_where_its_decode_starts() {
    let span = bae_db::SegmentSpan {
        start_sample: 44_100,
        end_sample: None,
        start_byte: Some(70_000),
        end_byte: None,
    };
    let first_read = |start_offset, start| {
        let buffer = filled_sparse_buffer(&[0; 16]);
        let id = buffer.id();
        StreamDecodeParams::new(
            vec![SegmentDecodeParams::with_start(buffer, span, start_offset, start)],
            true,
            0,
            0,
        )
        .first_read_byte(id)
    };
    let point = bae_db::ScrubPoint { sample: 88_200, byte: 300_000, exact_from: 88_200 };

    assert_eq!(first_read(0, SegmentStart::Span), Some(70_000));
    assert_eq!(first_read(50_000, SegmentStart::ScrubPoint(point)), Some(300_000));
    assert_eq!(first_read(50_000, SegmentStart::Span), None);
    assert_eq!(first_read(0, SegmentStart::StreamStart), None);
    assert_eq!(
        StreamDecodeParams::new(Vec::new(), true, 0, 0).first_read_byte(7),
        None,
        "no window reads through the buffer"
    );
}
