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
    span: bae_db::SegmentSpan,
    content_type: &bae_util::content_type::ContentType,
) -> Vec<bae_db::ScrubPoint> {
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
    use bae_db::ScrubPoints;
    use bae_db::{ScrubPoint, SegmentSpan};
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
        let point = points[next - 1];
        let target = points[next].sample - 1;

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

        let end = target + u64::from(SAMPLE_RATE);
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

mod mp3_scrub_points {
    //! A scrub in an MP3 decodes from the last stored scrub point that plays its
    //! target exactly, or from the stream's start before the first, and plays the
    //! samples a decode from the stream's start plays from the target, to the
    //! sample: in a CBR file at a normal and at a low bitrate, in a VBR file with
    //! a Xing table and without one, in mono, in MPEG-2 (22.05 kHz) and MPEG-2.5
    //! (8 kHz) files, in one shorter than the points' spacing, and in one larger
    //! than the window a scrub fetches.

    use super::*;
    use bae_audio::{encode_i32, EncodeFormat};
    use bae_db::ScrubPoints;
    use bae_db::{ScrubPoint, SegmentSpan};
    use bae_track_audio::data_source::FILL_WINDOW_SIZE;
    use bae_track_audio::{SegmentDecodeParams, SegmentStart, StreamDecodeParams};
    use bae_util::content_type::ContentType;

    /// A stream's sample rate and channel count.
    #[derive(Clone, Copy)]
    struct Layout {
        sample_rate: u32,
        channels: u32,
    }

    const STEREO_44K: Layout = Layout { sample_rate: 44_100, channels: 2 };

    impl Layout {
        fn second(self) -> u64 {
            u64::from(self.sample_rate)
        }

        /// Samples in one Layer III frame: MPEG-1's below 32 kHz are MPEG-2's
        /// and MPEG-2.5's half-length frames.
        fn frame(self) -> u64 {
            if self.sample_rate >= 32_000 {
                1_152
            } else {
                576
            }
        }
    }

    /// `seconds` of noise and a tone per channel under a slow swell, as
    /// interleaved i32: loud and quiet stretches, so a VBR encode varies its frame
    /// sizes and leans on its bit reservoir.
    fn signal(layout: Layout, seconds: u32) -> Vec<i32> {
        let mut state = 0x2545_f491_u32;
        let frames = layout.sample_rate * seconds;
        let mut samples = Vec::with_capacity((frames * layout.channels) as usize);
        for i in 0..frames {
            let t = f64::from(i) / f64::from(layout.sample_rate);
            let swell = 0.05 + 0.45 * (0.5 + 0.5 * (t * 0.4).sin());
            for channel in 0..layout.channels {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = f64::from(state as i32) / f64::from(i32::MAX);
                let pitch = 330.0 + 110.0 * f64::from(channel);
                let tone = (2.0 * std::f64::consts::PI * pitch * t).sin();
                let value = swell * (0.6 * noise + 0.4 * tone);
                samples.push((value * f64::from(i32::MAX)) as i32);
            }
        }
        samples
    }

    /// `samples` encoded by LAME at a constant bitrate, with the Xing/LAME header
    /// the MP3 muxer writes.
    fn cbr_mp3(samples: &[i32], layout: Layout, bitrate_kbps: u32) -> Vec<u8> {
        encode_i32(EncodeFormat::Mp3 { bitrate_kbps }, samples, layout.sample_rate, layout.channels)
            .expect("encode the CBR MP3")
    }

    /// `samples`, stereo at 44.1 kHz, encoded by LAME at VBR quality 2, with the
    /// Xing/LAME header the MP3 muxer writes or without one.
    fn vbr_mp3(samples: &[i32], xing: bool) -> Vec<u8> {
        use ffmpeg_next as ff;

        let sample_rate = STEREO_44K.sample_rate as i32;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("vbr.mp3");
        let codec = ff::encoder::find_by_name("libmp3lame").expect("the LAME encoder");
        let mut output = ff::format::output_as(&path, "mp3").expect("the MP3 muxer");
        let format = ff::format::Sample::I16(ff::format::sample::Type::Planar);
        let mut setup = ff::codec::context::Context::new_with_codec(codec)
            .encoder()
            .audio()
            .expect("an audio encoder");
        setup.set_rate(sample_rate);
        setup.set_channel_layout(ff::ChannelLayout::STEREO);
        setup.set_format(format);
        setup.set_time_base((1, sample_rate));
        setup.set_flags(ff::codec::Flags::QSCALE);
        // LAME's quality 2, in the lambda units FFmpeg's quality scale counts.
        // SAFETY: the context is allocated and not yet opened.
        unsafe {
            (*setup.as_mut_ptr()).global_quality = 2 * ff::ffi::FF_QP2LAMBDA;
        }
        let mut encoder = setup.open_as(codec).expect("open the encoder");
        output.add_stream(codec).expect("the MP3 stream").set_parameters(&encoder);
        let mut options = ff::Dictionary::new();
        if !xing {
            options.set("write_xing", "0");
        }
        output.write_header_with(options).expect("the MP3 header");
        let stream_time_base = output.stream(0).expect("the MP3 stream").time_base();

        let drain = |encoder: &mut ff::encoder::Audio, output: &mut ff::format::context::Output| {
            let mut packet = ff::Packet::empty();
            while encoder.receive_packet(&mut packet).is_ok() {
                packet.set_stream(0);
                packet.rescale_ts((1, sample_rate), stream_time_base);
                packet.write_interleaved(output).expect("write a packet");
            }
        };
        let frame_size = encoder.frame_size() as usize;
        let channels = STEREO_44K.channels as usize;
        for (index, chunk) in samples.chunks(frame_size * channels).enumerate() {
            let mut frame =
                ff::frame::Audio::new(format, chunk.len() / channels, ff::ChannelLayout::STEREO);
            for channel in 0..channels {
                for (i, sample) in frame.plane_mut::<i16>(channel).iter_mut().enumerate() {
                    *sample = (chunk[i * channels + channel] >> 16) as i16;
                }
            }
            frame.set_pts(Some((index * frame_size) as i64));
            encoder.send_frame(&frame).expect("encode a frame");
            drain(&mut encoder, &mut output);
        }
        encoder.send_eof().expect("flush the encoder");
        drain(&mut encoder, &mut output);
        output.write_trailer().expect("finish the MP3");
        std::fs::read(&path).expect("read the MP3 back")
    }

    /// One test file: its name, layout, length and bytes.
    struct Mp3 {
        name: &'static str,
        layout: Layout,
        seconds: u32,
        bytes: Vec<u8>,
    }

    impl Mp3 {
        fn cbr(name: &'static str, layout: Layout, seconds: u32, bitrate_kbps: u32) -> Self {
            Self {
                name,
                layout,
                seconds,
                bytes: cbr_mp3(&signal(layout, seconds), layout, bitrate_kbps),
            }
        }

        fn vbr(name: &'static str, xing: bool) -> Self {
            Self {
                name,
                layout: STEREO_44K,
                seconds: 60,
                bytes: vbr_mp3(&signal(STEREO_44K, 60), xing),
            }
        }
    }

    const MONO_44K: Layout = Layout { sample_rate: 44_100, channels: 1 };
    const STEREO_22K: Layout = Layout { sample_rate: 22_050, channels: 2 };
    const MONO_8K: Layout = Layout { sample_rate: 8_000, channels: 1 };

    /// 320 kbps for 4.5 minutes: 10.8 MB, more than two fetch windows.
    fn long_mp3() -> Mp3 {
        Mp3::cbr("CBR 320 kbps, 4.5 minutes", STEREO_44K, 270, 320)
    }

    /// Every file but the long one, by name.
    fn mp3s() -> Vec<Mp3> {
        vec![
            Mp3::cbr("CBR 128 kbps", STEREO_44K, 60, 128),
            Mp3::cbr("CBR 32 kbps", STEREO_44K, 60, 32),
            Mp3::vbr("VBR with a Xing table", true),
            Mp3::vbr("VBR with no table", false),
            Mp3::cbr("mono, CBR 64 kbps", MONO_44K, 60, 64),
            Mp3::cbr("MPEG-2, 22.05 kHz, CBR 32 kbps", STEREO_22K, 60, 32),
            Mp3::cbr("MPEG-2.5, 8 kHz mono, CBR 8 kbps", MONO_8K, 60, 8),
            Mp3::cbr("7 seconds, CBR 128 kbps", STEREO_44K, 7, 128),
        ]
    }

    /// Run `decode` to its end and return the samples it played.
    fn play(decode: StreamDecodeParams, layout: Layout) -> Vec<f32> {
        super::play_decode(decode, layout.sample_rate, layout.channels)
    }

    /// What a decode from the stream's first frame plays of the whole file.
    fn from_stream_start(mp3: &Mp3) -> Vec<f32> {
        play(
            StreamDecodeParams::new(
                vec![SegmentDecodeParams::with_start(
                    filled_sparse_buffer(&mp3.bytes),
                    SegmentSpan::whole_file(),
                    0,
                    SegmentStart::StreamStart,
                )],
                false,
                0,
                0,
            ),
            mp3.layout,
        )
    }

    /// Seek `mp3`, its bytes in `buffer` and `points` stored for it, `target`
    /// samples in through the playback service's seek mapping, the file ending
    /// at `end` so the decode stops there. Returns what played.
    fn scrub_from(
        buffer: SharedSparseBuffer,
        mp3: &Mp3,
        points: &[ScrubPoint],
        target: u64,
        end: Option<u64>,
    ) -> Vec<f32> {
        let mut prepared = test_prepared_track("track", buffer);
        prepared.content_type = ContentType::Mp3;
        prepared.sample_rate = mp3.layout.sample_rate;
        prepared.channels = mp3.layout.channels;
        prepared.segments[0].span = SegmentSpan { end_sample: end, ..SegmentSpan::whole_file() };
        prepared.segments[0].scrub_points = ScrubPoints::new(points.to_vec());
        play(prepared.decode_params(target, false), mp3.layout)
    }

    /// [`scrub_from`] over a buffer holding the whole file. Returns the offsets
    /// the demuxer read and what played.
    fn scrub(
        mp3: &Mp3,
        points: &[ScrubPoint],
        target: u64,
        end: Option<u64>,
    ) -> (Vec<u64>, Vec<f32>) {
        let buffer = filled_sparse_buffer(&mp3.bytes);
        let played = scrub_from(buffer.clone(), mp3, points, target, end);
        (buffer.read_log(), played)
    }

    /// Seek to each of `targets` in `mp3` with `points` stored, and check it
    /// plays what `reference`, the decode of the whole file from its start, plays
    /// from there: a second of it, or to the stream's end from a target within
    /// two seconds of it. Returns the targets whose samples differ.
    fn differing_targets(
        mp3: &Mp3,
        points: &[ScrubPoint],
        reference: &[f32],
        targets: &[u64],
    ) -> Vec<u64> {
        let channels = mp3.layout.channels as usize;
        let total = (reference.len() / channels) as u64;
        let second = mp3.layout.second();
        targets
            .iter()
            .copied()
            .filter(|&target| {
                let end = (target + 2 * second < total).then_some(target + second);
                let (_, played) = scrub(mp3, points, target, end);
                let stop = end.unwrap_or(total) as usize;
                played != reference[target as usize * channels..stop * channels]
            })
            .collect()
    }

    /// At each point, at the first sample it plays exactly and the samples on
    /// either side of it, at the point's own frame, at fixed places through the
    /// file, and at its last samples.
    fn targets(points: &[ScrubPoint], total: u64, layout: Layout) -> Vec<u64> {
        let second = layout.second();
        let mut targets: Vec<u64> = [
            0,
            1,
            second / 10,
            5 * second + 7,
            10 * second,
            10 * second + 1,
            20 * second - 1,
            25 * second + second / 2,
            40 * second + 12_345,
            59 * second + second / 2,
            total - second / 2,
            total - 1,
        ]
        .into_iter()
        .filter(|&target| target < total)
        .collect();
        for point in points {
            targets.extend([
                point.sample,
                point.exact_from - 1,
                point.exact_from,
                point.exact_from + 1,
            ]);
        }
        targets.sort_unstable();
        targets.dedup();
        targets
    }

    /// Check `mp3`'s stored points, and that a scrub to each target plays the
    /// stream's own samples and reads only the file's head and the bytes from
    /// its point.
    fn check_scrubs(mp3: &Mp3) {
        let name = mp3.name;
        let points = import_scrub_points(&mp3.bytes, SegmentSpan::whole_file(), &ContentType::Mp3);
        let reference = from_stream_start(mp3);
        let total = (reference.len() / mp3.layout.channels as usize) as u64;

        // A point for a frame about each 10 seconds in, the last of them
        // within a frame or so of the file's end; the first 10 seconds decode
        // from the stream's start. Each plays exactly some frames past its own.
        let ten_seconds = 10 * mp3.layout.second();
        let frame = mp3.layout.frame();
        let steps = mp3.seconds / 10;
        assert!(
            points.len() as u32 == steps || points.len() as u32 + 1 == steps,
            "{name}: {points:?}"
        );
        assert!(points.iter().all(|point| point.sample < point.exact_from), "{name}: {points:?}");
        if let Some(first) = points.first() {
            assert!(first.exact_from <= ten_seconds + frame, "{name}: {first:?}");
        }
        for pair in points.windows(2) {
            let gap = pair[1].exact_from - pair[0].exact_from;
            assert!(
                gap >= ten_seconds - 3 * frame && gap < ten_seconds + 3 * frame,
                "{name}: points {pair:?} are {gap} samples apart"
            );
        }

        // A scrub to the first sample a point plays exactly reads the file's
        // head (the probe's reads, from byte 0 on), then from the point on:
        // nothing between.
        let avio = bae_audio::AVIO_BUFFER_SIZE as u64;
        for point in &points {
            let end = Some(point.exact_from + mp3.layout.second());
            let (reads, _) = scrub(mp3, &points, point.exact_from, end);
            let head = reads.iter().take_while(|&&at| at + avio <= point.byte).count();
            assert!(
                reads[..head].iter().enumerate().all(|(index, &at)| at == index as u64 * avio)
                    && reads[head..].iter().all(|&at| at + avio > point.byte),
                "{name}: reads {reads:?} leave the head and the bytes from {point:?}"
            );
        }

        let differing =
            differing_targets(mp3, &points, &reference, &targets(&points, total, mp3.layout));
        assert!(
            differing.is_empty(),
            "{name}: scrubs to {differing:?} play other samples than the stream's own, points {points:?}"
        );
    }

    #[test]
    fn a_scrub_in_an_mp3_plays_the_streams_own_samples_from_its_target() {
        bae_audio::init();
        for mp3 in mp3s() {
            check_scrubs(&mp3);
        }
    }

    #[test]
    fn a_scrub_in_an_mp3_larger_than_a_window_plays_the_streams_own_samples() {
        bae_audio::init();
        let mp3 = long_mp3();
        assert!(mp3.bytes.len() as u64 > 2 * FILL_WINDOW_SIZE);
        check_scrubs(&mp3);
    }

    /// `bytes` without the ID3v2 tag the MP3 muxer writes at its start: a file
    /// FFmpeg finds no tag in, so it looks for an ID3v1 tag in the file's last
    /// 128 bytes when it opens it.
    fn without_id3v2(bytes: &[u8]) -> Vec<u8> {
        assert_eq!(&bytes[..3], b"ID3", "the muxer writes an ID3v2 tag");
        let size =
            bytes[6..10].iter().fold(0usize, |size, &byte| (size << 7) | usize::from(byte & 0x7f));
        let footer = if bytes[5] & 0x10 != 0 { 10 } else { 0 };
        bytes[10 + size + footer..].to_vec()
    }

    /// Scrub `mp3` to `target` through a buffer a fill fetches from its bytes,
    /// and return what played and the byte range of each fetch the fill had made
    /// when the decode ended, a second after the target, in the order it made
    /// them.
    async fn scrub_fetching(
        mp3: &Mp3,
        points: &[ScrubPoint],
        target: u64,
    ) -> (Vec<f32>, Vec<(u64, u64)>) {
        let bytes = Arc::new(mp3.bytes.clone());
        let buffer = create_sparse_buffer(bytes.len() as u64);
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let fill = {
            let buffer = buffer.clone();
            let log = log.clone();
            tokio::spawn(async move {
                buffer
                    .fill_on_demand(FILL_WINDOW_SIZE, move |start, len| {
                        let bytes = bytes.clone();
                        let log = log.clone();
                        async move {
                            log.lock().unwrap().push((start, start + len));
                            Ok::<_, bae_track_audio::TrackReadError>(
                                bytes[start as usize..(start + len) as usize].to_vec(),
                            )
                        }
                    })
                    .await
                    .expect("the fill reads the file")
            })
        };
        let end = Some(target + mp3.layout.second());
        let played = {
            let buffer = buffer.clone();
            let points = points.to_vec();
            let mp3 =
                Mp3 { name: mp3.name, layout: mp3.layout, seconds: mp3.seconds, bytes: Vec::new() };
            tokio::task::spawn_blocking(move || scrub_from(buffer, &mp3, &points, target, end))
                .await
                .expect("the scrub")
        };
        let ranges = log.lock().unwrap().clone();
        buffer.cancel();
        fill.await.expect("the fill task");
        (played, ranges)
    }

    /// The cloud cost of a scrub deep into a file larger than a window: the
    /// file's head, which the probe reads, and one window from the point, but
    /// none of the windows between. A file with no ID3v2 tag also costs a fetch
    /// of its last 128 bytes, where FFmpeg looks for an ID3v1 tag on open, but
    /// no second fetch of the head the open comes back to.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_scrub_deep_into_a_long_mp3_fetches_its_head_and_the_window_from_its_point() {
        bae_audio::init();
        let tagged = long_mp3();
        let untagged = Mp3 {
            name: "CBR 320 kbps, 4.5 minutes, no ID3v2 tag",
            bytes: without_id3v2(&tagged.bytes),
            ..long_mp3()
        };
        for mp3 in [&tagged, &untagged] {
            let name = mp3.name;
            let size = mp3.bytes.len() as u64;
            let points =
                import_scrub_points(&mp3.bytes, SegmentSpan::whole_file(), &ContentType::Mp3);
            let point = *points.last().expect("a long file has points");
            assert!(point.byte > 2 * FILL_WINDOW_SIZE, "{name}: {point:?}");
            let reference = from_stream_start(mp3);
            let channels = mp3.layout.channels as usize;
            let target = point.exact_from;

            let (played, fetches) = scrub_fetching(mp3, &points, target).await;
            let stop = target + mp3.layout.second();
            assert!(
                played == reference[target as usize * channels..stop as usize * channels],
                "{name}: the scrub plays other samples than the stream's own"
            );
            // The head window the probe reads, and the fill's read-ahead past it
            // as the probe reads on (one window ahead of the reader until the
            // decode seeks), up to the probe's last read.
            let head_end = FILL_WINDOW_SIZE + 4 * bae_audio::AVIO_BUFFER_SIZE as u64;
            let id3v1 = (size - 128, size);
            let tagged = mp3.bytes.starts_with(b"ID3");
            assert_eq!(fetches[0].0, 0, "{name}: the first fetch is the head");
            assert!(
                fetches.iter().all(|&range| range.1 <= head_end
                    || range.0 >= point.byte
                    || (!tagged && range == id3v1)),
                "{name}: the scrub fetched between the head and the point: {fetches:?}"
            );
            assert_eq!(
                fetches.contains(&id3v1),
                !tagged,
                "{name}: the file's last 128 bytes are fetched only without an ID3v2 tag"
            );
            // The buffer keeps the file's first window and the read-ahead past it
            // for the probe's first read, so the open's jump to the last 128 bytes
            // and back fetches neither again: no byte is fetched twice.
            let mut ordered = fetches.clone();
            ordered.sort_unstable();
            assert!(
                ordered.windows(2).all(|pair| pair[0].1 <= pair[1].0),
                "{name}: a byte is fetched twice: {fetches:?}"
            );
            // Besides the read-ahead past the head (how much of it the fill makes
            // before the decode seeks depends on how far ahead of the fill the
            // probe reads), the scrub fetches the head window, the last 128 bytes
            // when there is no ID3v2 tag, and the window from the point.
            let read_aheads = fetches
                .iter()
                .filter(|&&(start, end)| start >= FILL_WINDOW_SIZE && end <= head_end)
                .count();
            assert_eq!(
                fetches.len() - read_aheads,
                if tagged { 2 } else { 3 },
                "{name}: {fetches:?}"
            );
        }
    }

    /// Every packet a decode from the stream's start reads.
    struct Packets(Vec<bae_audio::DecodedPacket>);

    impl bae_audio::DecodedSink for Packets {
        fn on_format(&mut self, _sample_rate: u32, _channels: u32) {}
        fn on_samples(&mut self, _samples: &[i32]) {}
        fn on_packet(&mut self, packet: bae_audio::DecodedPacket) {
            self.0.push(packet);
        }
    }

    /// The points import stores lean on the frames before them as far as the bit
    /// reservoir reaches. Each moved one frame later, keeping the sample it
    /// claims to play exactly from, a scrub there plays other samples than the
    /// stream's own: at 128 kbps two frames' main data are the least that holds a
    /// full reservoir, and the frame before the one a point serves must decode
    /// whole.
    #[test]
    fn a_point_one_frame_short_of_the_reservoir_plays_other_samples() {
        bae_audio::init();
        let mp3 = Mp3::cbr("CBR 128 kbps", STEREO_44K, 60, 128);
        let points = import_scrub_points(&mp3.bytes, SegmentSpan::whole_file(), &ContentType::Mp3);
        let reference = from_stream_start(&mp3);

        let mut packets = Packets(Vec::new());
        bae_audio::decode_audio_to_sink(
            filled_sparse_buffer(&mp3.bytes),
            None,
            None,
            &mut packets,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .expect("read the MP3's packets");
        let later: Vec<ScrubPoint> = points
            .iter()
            .map(|point| {
                let next = packets
                    .0
                    .iter()
                    .find(|packet| packet.byte.is_some_and(|byte| byte > point.byte))
                    .expect("a frame after the point");
                ScrubPoint {
                    sample: u64::try_from(next.sample.expect("a timestamp")).unwrap(),
                    byte: next.byte.unwrap(),
                    exact_from: point.exact_from,
                }
            })
            .collect();

        let exact_froms: Vec<u64> = points.iter().map(|point| point.exact_from).collect();
        assert!(
            differing_targets(&mp3, &points, &reference, &exact_froms).is_empty(),
            "the stored points play exactly"
        );
        assert_eq!(
            differing_targets(&mp3, &later, &reference, &exact_froms),
            exact_froms,
            "a point one frame later plays exactly anyway"
        );
    }
}

mod image_track_starts {
    //! A track of a single-file image starts where import places it, at the byte
    //! a seek to its first sample lands on, whatever the image's codec: playback
    //! jumps by byte only to a frame that states its own sample, and seeks by
    //! sample to any other.

    use super::*;
    use bae_audio::EncodeFormat;
    use bae_db::SegmentSpan;
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
