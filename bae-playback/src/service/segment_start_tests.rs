//! Where a decode inside a segment starts, checked by what it plays: a scrub
//! from a stored point plays the samples a decode from the start plays, and
//! a single-file image's track starts at its first sample whatever its codec.

use crate::service::tests::*;
use crate::service::*;
use bae_audio::test_samples::filled_sparse_buffer;

/// Takes a verifying decode's scrub points as import's sink does, and
/// discards its samples.
struct TakesScrubPoints(Option<bae_track_audio::scrub_points::ScrubPointTaker>);

impl bae_audio::DecodedSink for TakesScrubPoints {
    fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
    fn on_samples(&mut self, _samples: &[i32]) {}
    fn on_stream(&mut self, stream: &bae_audio::DecodedStream) {
        self.0 = bae_track_audio::scrub_points::ScrubPointTaker::for_stream(stream);
    }
    fn on_packet(&mut self, packet: bae_audio::DecodedPacket) {
        if let Some(taker) = self.0.as_mut() {
            taker.observe(packet);
        }
    }
}

/// The scrub points the import's verifying decode takes over `span` of a
/// `content_type` file holding `bytes`, which must be one that gets them.
fn import_scrub_points(
    bytes: &[u8],
    span: bae_records::SegmentSpan,
    content_type: &bae_util::content_type::ContentType,
) -> Vec<bae_records::ScrubPoint> {
    use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};
    let decode = StreamDecodeParams::new(
        vec![SegmentDecodeParams::new(filled_sparse_buffer(bytes), span, 0)],
        bae_audio::byte_jump_keeps_place(content_type),
        0,
        0,
    );
    let mut sink = TakesScrubPoints(None);
    decode
        .verify_to_sink(&mut sink, Arc::new(std::sync::atomic::AtomicBool::new(false)))
        .expect("the verifying decode reads the file");
    sink.0.expect("the file is one that gets scrub points").finish()
}

mod scrub_points {
    //! A scrub inside a FLAC with no seektable jumps by byte to the stored scrub
    //! point at or before its target, reads within one fill window from there,
    //! and plays the samples a decode from the track's start trimmed to the target
    //! plays.

    use super::*;
    use bae_audio::{encode_i32, EncodeFormat, AVIO_BUFFER_SIZE};
    use bae_records::ScrubPoints;
    use bae_records::{ScrubPoint, SegmentSpan};
    use bae_track_audio::data_source::FILL_WINDOW_SIZE;
    use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};
    use bae_util::content_type::ContentType;

    const SAMPLE_RATE: u32 = 96_000;
    const CHANNELS: u32 = 2;

    /// Seconds of 24-bit stereo noise at 96 kHz, encoded as a FLAC with no
    /// seektable (FFmpeg's FLAC muxer writes none). Noise barely compresses, so
    /// ten seconds outgrow one fill window and the byte rule spaces the points.
    fn noise_flac(seconds: u32) -> Vec<u8> {
        let mut state = 0x2545_f491_u32;
        let samples: Vec<i32> = (0..SAMPLE_RATE * CHANNELS * seconds)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as i32 >> 8) << 8
            })
            .collect();
        encode_i32(EncodeFormat::Flac { bits_per_sample: 24 }, &samples, SAMPLE_RATE, CHANNELS)
            .expect("encode the noise FLAC")
    }

    /// The scrub points the import's verifying decode takes over `span` of `flac`.
    fn import_scrub_points(flac: &[u8], span: SegmentSpan) -> Vec<ScrubPoint> {
        super::import_scrub_points(flac, span, &ContentType::Flac)
    }

    /// Run `decode` to its end and return the samples it played.
    fn play(decode: StreamDecodeParams) -> Vec<f32> {
        super::play_decode(decode, SAMPLE_RATE, CHANNELS)
    }

    /// Scrub `segment` of `flac`, with `points` stored for it, to `target`
    /// through the playback service's seek mapping, the track ending at `end` so
    /// the decode stops there. Returns the offsets the demuxer read and what
    /// played.
    fn scrub(
        flac: &[u8],
        segment: SegmentSpan,
        points: &[ScrubPoint],
        target: u64,
        end: u64,
    ) -> (Vec<u64>, Vec<f32>) {
        let buffer = filled_sparse_buffer(flac);
        let mut prepared = test_prepared_track("track", buffer.clone());
        prepared.sample_rate = SAMPLE_RATE;
        prepared.segments[0].span = SegmentSpan { end_sample: Some(end), ..segment };
        prepared.segments[0].scrub_points = ScrubPoints::new(points.to_vec());
        let played = play(prepared.decode_params(target - segment.start_sample, false));
        (buffer.read_log(), played)
    }

    /// What a decode that jumps to the track's first frame and trims to `target`
    /// plays up to `end`.
    fn from_track_start(flac: &[u8], first_frame: u64, target: u64, end: u64) -> Vec<f32> {
        let window = SegmentSpan {
            start_sample: target,
            end_sample: Some(end),
            start_byte: Some(first_frame),
            end_byte: None,
        };
        play(StreamDecodeParams::new(
            vec![SegmentDecodeParams::new(filled_sparse_buffer(flac), window, 0)],
            true,
            0,
            0,
        ))
    }

    /// Scrub to the last sample before `points[next]`, the farthest any sample
    /// sits from the point that serves it, and check the scrub's reads and
    /// samples.
    fn check_worst_scrub(flac: &[u8], segment: SegmentSpan, points: &[ScrubPoint], next: usize) {
        let target = points[next].sample - 1;
        check_scrub(
            flac,
            segment,
            points,
            points[next - 1],
            target,
            target + u64::from(SAMPLE_RATE),
        );
    }

    /// Scrub to `target`, which `point` serves, and check that the scrub reads
    /// only the file's head and one window from the point, and plays what a
    /// decode from the track's start plays from `target` to `end`.
    fn check_scrub(
        flac: &[u8],
        segment: SegmentSpan,
        points: &[ScrubPoint],
        point: ScrubPoint,
        target: u64,
        end: u64,
    ) {
        // Handing over the target's frame, the scrub reads two runs: the file's
        // head (the probe of its header and first frames) and, from the point,
        // what one fill window holds. Nothing else before the point, and never the
        // file's end.
        let (reads, _) = scrub(flac, segment, points, target, target + 1);
        let in_head = |at: u64| at + AVIO_BUFFER_SIZE as u64 <= FILL_WINDOW_SIZE;
        let in_jump = |at: u64| {
            at >= point.byte && at + AVIO_BUFFER_SIZE as u64 <= point.byte + FILL_WINDOW_SIZE
        };
        assert!(
            reads.iter().all(|&at| in_head(at) || in_jump(at)),
            "reads {reads:?} leave the head window and the window from the point {point:?}"
        );
        assert!(
            reads.iter().any(|&at| at >= point.byte),
            "the scrub never read from the point: {reads:?}"
        );

        let (_, played) = scrub(flac, segment, points, target, end);
        let reference = from_track_start(flac, points[0].byte, target, end);
        assert_eq!(played.len(), (end - target) as usize * CHANNELS as usize);
        assert!(
            played == reference,
            "the scrub's samples differ from a decode from the track's start"
        );
    }

    #[test]
    fn a_scrub_in_a_file_without_a_seektable_jumps_to_its_scrub_point() {
        bae_audio::init();
        let flac = noise_flac(30);
        let whole = SegmentSpan::whole_file();
        let points = import_scrub_points(&flac, whole);
        assert!(points.len() >= 5, "30 s of noise spans several windows: {points:?}");
        // Spaced by bytes, not by the 10 s rule: ten seconds of this noise
        // outgrow one window.
        for pair in points.windows(2) {
            assert!(
                pair[1].byte - pair[0].byte <= FILL_WINDOW_SIZE
                    && pair[1].sample - pair[0].sample < 10 * u64::from(SAMPLE_RATE),
                "points {pair:?}"
            );
        }
        check_worst_scrub(&flac, whole, &points, 3);
        check_worst_scrub(&flac, whole, &points, points.len() - 1);
    }

    /// A scrub past a segment's last point, into its final frames, which the
    /// binary search FFmpeg falls back to without a seektable can't place,
    /// starts at that point and decodes forward: it reads the file's head and
    /// one window from the point, nowhere else.
    #[test]
    fn a_scrub_past_the_last_point_reads_on_from_it() {
        bae_audio::init();
        let flac = noise_flac(30);
        let whole = SegmentSpan::whole_file();
        let points = import_scrub_points(&flac, whole);
        let last = *points.last().expect("30 s of noise gets points");
        let total = 30 * u64::from(SAMPLE_RATE);
        for target in [last.sample + 1, total - 10] {
            check_scrub(&flac, whole, &points, last, target, total);
        }
    }

    /// A track that starts inside the file, as a CUE album's do: its decode starts
    /// at the byte import recorded, and its first point is the frame its first
    /// sample lies in.
    #[test]
    fn a_scrub_inside_a_track_of_a_single_file_album_jumps_to_its_scrub_point() {
        bae_audio::init();
        let flac = noise_flac(30);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("album.flac");
        std::fs::write(&path, &flac).unwrap();
        let start_sample = 12 * u64::from(SAMPLE_RATE) + 1_234;
        let start_byte = bae_audio::seek_landing_bytes(&path, &[start_sample])
            .expect("the track's landing byte")[0];
        let track = SegmentSpan {
            start_sample,
            end_sample: None,
            start_byte: Some(start_byte),
            end_byte: None,
        };
        let points = import_scrub_points(&flac, track);
        assert_eq!(points[0].byte, start_byte, "the first point is the track's own frame");
        assert!(points[0].sample <= start_sample && points[1].sample > start_sample);
        check_worst_scrub(&flac, track, &points, 2);
    }

    /// A track that starts inside a single-file FLAC with no seektable starts
    /// by jumping to the byte import recorded for it: it reads the file's head
    /// and the window from that byte, never the file's end that FFmpeg's
    /// binary search, its only seek in such a file, reads first (#226).
    #[test]
    fn a_track_start_in_a_file_without_a_seektable_jumps_to_its_byte() {
        bae_audio::init();
        let flac = noise_flac(30);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("album.flac");
        std::fs::write(&path, &flac).unwrap();
        let start_sample = 12 * u64::from(SAMPLE_RATE) + 1_234;
        let start_byte = bae_audio::seek_landing_bytes(&path, &[start_sample])
            .expect("the track's landing byte")[0];
        let end = start_sample + u64::from(SAMPLE_RATE);
        let track = SegmentSpan {
            start_sample,
            end_sample: Some(end),
            start_byte: Some(start_byte),
            end_byte: None,
        };
        let buffer = filled_sparse_buffer(&flac);
        let mut prepared = test_prepared_track("track", buffer.clone());
        prepared.sample_rate = SAMPLE_RATE;
        prepared.segments[0].span = track;
        prepared.segments[0].scrub_points =
            ScrubPoints::new(import_scrub_points(&flac, SegmentSpan { end_sample: None, ..track }));

        let played = play(prepared.decode_params(0, false));

        assert_eq!(played.len(), (end - start_sample) as usize * CHANNELS as usize);
        let reads = buffer.read_log();
        let in_head = |at: u64| at + AVIO_BUFFER_SIZE as u64 <= FILL_WINDOW_SIZE;
        let in_jump = |at: u64| {
            at >= start_byte && at + AVIO_BUFFER_SIZE as u64 <= start_byte + FILL_WINDOW_SIZE
        };
        assert!(
            reads.iter().all(|&at| in_head(at) || in_jump(at)),
            "reads {reads:?} leave the head window and the window from byte {start_byte} of {}",
            flac.len()
        );
    }

    /// Seconds of quiet 24-bit stereo tone at 96 kHz, encoded as a FLAC with no
    /// seektable, `frames` samples per channel long. It compresses well, so ten
    /// seconds fit in one fill window and time alone spaces the points.
    fn quiet_flac(frames: u64) -> Vec<u8> {
        let samples: Vec<i32> = (0..frames * u64::from(CHANNELS))
            .map(|i| ((i as f64 * 0.001).sin() * 0.01 * i32::MAX as f64) as i32)
            .collect();
        encode_i32(EncodeFormat::Flac { bits_per_sample: 24 }, &samples, SAMPLE_RATE, CHANNELS)
            .expect("encode the quiet FLAC")
    }

    /// A file whose last frame is the first one ten seconds past its first point,
    /// so the spacing would put a point there. A decode that jumps to a FLAC's
    /// last frame can't date it and so can't trim to a target in it: a scrub into
    /// that frame must still play exactly from its target.
    #[test]
    fn a_scrub_into_the_last_frame_plays_from_its_target() {
        bae_audio::init();
        // STREAMINFO follows the 4-byte marker and its 4-byte block header; its
        // maximum block size is bytes 2..4, which this encoder uses for every
        // frame but the last.
        let pilot = quiet_flac(u64::from(SAMPLE_RATE));
        let block = u64::from(u16::from_be_bytes([pilot[10], pilot[11]]));
        let ten_seconds = 10 * u64::from(SAMPLE_RATE);
        let last_frame_start = ten_seconds.div_ceil(block) * block;
        let total = last_frame_start + 1_000;
        let flac = quiet_flac(total);
        let whole = SegmentSpan::whole_file();
        let points = import_scrub_points(&flac, whole);

        let target = total - 10;
        let (_, played) = scrub(&flac, whole, &points, target, total);
        let reference = from_track_start(&flac, points[0].byte, target, total);
        assert_eq!(
            played.len(),
            (total - target) as usize * CHANNELS as usize,
            "the scrub plays from its target, points {points:?}"
        );
        assert!(
            played == reference,
            "the scrub's samples differ from a decode from the track's start"
        );
    }
}

#[path = "segment_start_mp3_tests.rs"]
mod mp3_scrub_points;

mod image_track_starts {
    //! A track of a single-file image starts where import places it, at the byte
    //! a seek to its first sample lands on, whatever the image's codec: playback
    //! jumps by byte only to a frame that states its own sample, and seeks by
    //! sample to any other.

    use super::*;
    use bae_audio::EncodeFormat;
    use bae_records::SegmentSpan;
    use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};
    use bae_util::content_type::ContentType;

    /// The sample a CUE sheet's track 2 starts at in the images these tests
    /// play: three seconds in.
    const TRACK_2_START: u64 = 3 * 44_100;

    /// Ten seconds of a stereo tone encoded as `format`, written into `dir` as
    /// `name`, and its track 2 as import stores it: from [`TRACK_2_START`] to
    /// the image's end, starting its decode at the byte a seek to that sample
    /// lands on.
    fn image_track2(
        dir: &std::path::Path,
        name: &str,
        format: EncodeFormat,
    ) -> (Vec<u8>, SegmentSpan) {
        let samples: Vec<i32> = (0..44_100 * 10 * 2)
            .map(|i| (((i / 2) as f64 * 0.031).sin() * 0.3 * i32::MAX as f64) as i32)
            .collect();
        let bytes = bae_audio::encode_i32(format, &samples, 44_100, 2).expect("encode the image");
        let path = dir.join(name);
        std::fs::write(&path, &bytes).expect("write the image");
        let landing = bae_audio::seek_landing_bytes(&path, &[TRACK_2_START])
            .expect("read where track 2 starts")
            .pop();
        let span = SegmentSpan {
            start_sample: TRACK_2_START,
            end_sample: None,
            start_byte: landing,
            end_byte: None,
        };
        (bytes, span)
    }

    /// Start track 2 of a `format` image as playback does on a natural start,
    /// and return what it played and what a decode of the whole image plays from
    /// the track's first sample on.
    fn track2_and_reference(
        name: &str,
        format: EncodeFormat,
        content_type: ContentType,
    ) -> (Vec<f32>, Vec<f32>) {
        let temp = tempfile::tempdir().expect("tempdir");
        let (bytes, track2) = image_track2(temp.path(), name, format);
        let buffer = || filled_sparse_buffer(&bytes);

        let mut prepared = test_prepared_track("track-2", buffer());
        prepared.content_type = content_type;
        prepared.segments[0].span = track2;
        let played = play_decode(prepared.decode_params(0, false), 44_100, 2);

        let whole = play_decode(
            StreamDecodeParams::new(
                vec![SegmentDecodeParams::new(buffer(), SegmentSpan::whole_file(), 0)],
                true,
                0,
                0,
            ),
            44_100,
            2,
        );
        let reference = whole[track2.start_sample as usize * 2..].to_vec();
        (played, reference)
    }

    /// A WAV image's track starts at its first sample: after a jump by byte, the
    /// WAV demuxer restarts its timestamps at 0, so the track seeks by sample.
    #[test]
    fn a_wav_image_track_starts_at_its_first_sample() {
        bae_audio::init();
        let (played, reference) = track2_and_reference(
            "image.wav",
            EncodeFormat::PcmWav { bits_per_sample: 16 },
            ContentType::Pcm,
        );
        assert_eq!(played.len(), reference.len(), "sample count");
        assert!(played == reference, "the track's samples differ from the image's");
    }

    /// An MP3 image's track starts at its first sample: after a jump by byte, the
    /// MP3 demuxer restarts its timestamps at 0, so the track seeks by sample. Its
    /// first frame's samples come out once the decoder has its bit reservoir, a
    /// frame or two in, so only the count and what follows are compared.
    #[test]
    fn an_mp3_image_track_starts_at_its_first_sample() {
        bae_audio::init();
        let (played, reference) = track2_and_reference(
            "image.mp3",
            EncodeFormat::Mp3 { bitrate_kbps: 192 },
            ContentType::Mp3,
        );
        assert_eq!(played.len(), reference.len(), "sample count");
        let settled = 2 * 1_152 * 2;
        assert!(
            played[settled..] == reference[settled..],
            "the track's samples differ from the image's past its first frames"
        );
    }

    /// An AAC (MP4) image's track starts at its first sample: MP4's demuxer can't
    /// jump by byte, so the track seeks by sample through its sample table.
    #[test]
    fn an_aac_image_track_starts_at_its_first_sample() {
        bae_audio::init();
        let (played, reference) = track2_and_reference(
            "image.m4a",
            EncodeFormat::Aac { bitrate_kbps: 192 },
            ContentType::Aac,
        );
        assert_eq!(played.len(), reference.len(), "sample count");
        assert!(played == reference, "the track's samples differ from the image's");
    }
}
