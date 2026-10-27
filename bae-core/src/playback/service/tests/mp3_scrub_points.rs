//! A scrub in an MP3 decodes from the last stored scrub point that plays its
//! target exactly, or from the stream's start before the first, and plays the
//! samples a decode from the stream's start plays from the target, to the
//! sample: in a CBR file at a normal and at a low bitrate, in a VBR file with
//! a Xing table and without one, in mono, in MPEG-2 (22.05 kHz) and MPEG-2.5
//! (8 kHz) files, in one shorter than the points' spacing, and in one larger
//! than the window a scrub fetches.

use super::*;
use crate::db::{ScrubPoint, SegmentSpan};
use crate::playback::data_source::FILL_WINDOW_SIZE;
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use crate::playback::ScrubPoints;
use bae_audio::{encode_i32, EncodeFormat};
use bae_util::content_type::ContentType;

/// A stream's sample rate and channel count.
#[derive(Clone, Copy)]
struct Layout {
    sample_rate: u32,
    channels: u32,
}

const STEREO_44K: Layout = Layout {
    sample_rate: 44_100,
    channels: 2,
};

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
    encode_i32(
        EncodeFormat::Mp3 { bitrate_kbps },
        samples,
        layout.sample_rate,
        layout.channels,
    )
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
    output
        .add_stream(codec)
        .expect("the MP3 stream")
        .set_parameters(&encoder);
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

const MONO_44K: Layout = Layout {
    sample_rate: 44_100,
    channels: 1,
};
const STEREO_22K: Layout = Layout {
    sample_rate: 22_050,
    channels: 2,
};
const MONO_8K: Layout = Layout {
    sample_rate: 8_000,
    channels: 1,
};

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
            vec![SegmentDecodeParams::from_stream_start(
                filled_buffer(&mp3.bytes),
                SegmentSpan::whole_file(),
                0,
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
    prepared.segments[0].span = SegmentSpan {
        end_sample: end,
        ..SegmentSpan::whole_file()
    };
    prepared.segments[0].scrub_points = ScrubPoints::new(points.to_vec());
    play(prepared.decode_params(target, false), mp3.layout)
}

/// [`scrub_from`] over a buffer holding the whole file. Returns the offsets
/// the demuxer read and what played.
fn scrub(mp3: &Mp3, points: &[ScrubPoint], target: u64, end: Option<u64>) -> (Vec<u64>, Vec<f32>) {
    let buffer = filled_buffer(&mp3.bytes);
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
    assert!(
        points.iter().all(|point| point.sample < point.exact_from),
        "{name}: {points:?}"
    );
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
        let head = reads
            .iter()
            .take_while(|&&at| at + avio <= point.byte)
            .count();
        assert!(
            reads[..head]
                .iter()
                .enumerate()
                .all(|(index, &at)| at == index as u64 * avio)
                && reads[head..].iter().all(|&at| at + avio > point.byte),
            "{name}: reads {reads:?} leave the head and the bytes from {point:?}"
        );
    }

    let differing = differing_targets(
        mp3,
        &points,
        &reference,
        &targets(&points, total, mp3.layout),
    );
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
    let size = bytes[6..10]
        .iter()
        .fold(0usize, |size, &byte| (size << 7) | usize::from(byte & 0x7f));
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
                        Ok::<_, crate::playback::error::PlaybackError>(
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
        let mp3 = Mp3 {
            name: mp3.name,
            layout: mp3.layout,
            seconds: mp3.seconds,
            bytes: Vec::new(),
        };
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
/// of its last 128 bytes, where FFmpeg looks for an ID3v1 tag on open.
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
        let points = import_scrub_points(&mp3.bytes, SegmentSpan::whole_file(), &ContentType::Mp3);
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
        filled_buffer(&mp3.bytes),
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
