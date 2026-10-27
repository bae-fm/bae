//! A scrub in an MP3 decodes from the last stored scrub point that plays its
//! target exactly, or from the stream's start before the first, and plays the
//! samples a decode from the stream's start plays from the target, to the
//! sample: in a CBR file at a normal and at a low bitrate, and in a VBR file
//! with a Xing table and without one.

use super::*;
use crate::db::{ScrubPoint, SegmentSpan};
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use crate::playback::ScrubPoints;
use bae_audio::{encode_i32, EncodeFormat};
use bae_util::content_type::ContentType;

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
const SECONDS: u32 = 60;

/// `SECONDS` of stereo noise and two tones under a slow swell, as interleaved
/// i32: loud and quiet stretches, so a VBR encode varies its frame sizes and
/// leans on its bit reservoir.
fn signal() -> Vec<i32> {
    let mut state = 0x2545_f491_u32;
    let frames = SAMPLE_RATE * SECONDS;
    let mut samples = Vec::with_capacity((frames * CHANNELS) as usize);
    for i in 0..frames {
        let t = f64::from(i) / f64::from(SAMPLE_RATE);
        let swell = 0.05 + 0.45 * (0.5 + 0.5 * (t * 0.4).sin());
        for channel in 0..CHANNELS {
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
fn cbr_mp3(samples: &[i32], bitrate_kbps: u32) -> Vec<u8> {
    encode_i32(
        EncodeFormat::Mp3 { bitrate_kbps },
        samples,
        SAMPLE_RATE,
        CHANNELS,
    )
    .expect("encode the CBR MP3")
}

/// `samples` encoded by LAME at VBR quality 2, with the Xing/LAME header the
/// MP3 muxer writes or without one.
fn vbr_mp3(samples: &[i32], xing: bool) -> Vec<u8> {
    use ffmpeg_next as ff;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("vbr.mp3");
    let codec = ff::encoder::find_by_name("libmp3lame").expect("the LAME encoder");
    let mut output = ff::format::output_as(&path, "mp3").expect("the MP3 muxer");
    let format = ff::format::Sample::I16(ff::format::sample::Type::Planar);
    let mut setup = ff::codec::context::Context::new_with_codec(codec)
        .encoder()
        .audio()
        .expect("an audio encoder");
    setup.set_rate(SAMPLE_RATE as i32);
    setup.set_channel_layout(ff::ChannelLayout::STEREO);
    setup.set_format(format);
    setup.set_time_base((1, SAMPLE_RATE as i32));
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
            packet.rescale_ts((1, SAMPLE_RATE as i32), stream_time_base);
            packet.write_interleaved(output).expect("write a packet");
        }
    };
    let frame_size = encoder.frame_size() as usize;
    let channels = CHANNELS as usize;
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

/// The four files, by name.
fn mp3s() -> Vec<(&'static str, Vec<u8>)> {
    let samples = signal();
    vec![
        ("CBR 128 kbps", cbr_mp3(&samples, 128)),
        ("CBR 32 kbps", cbr_mp3(&samples, 32)),
        ("VBR with a Xing table", vbr_mp3(&samples, true)),
        ("VBR with no table", vbr_mp3(&samples, false)),
    ]
}

/// Run `decode` to its end and return the samples it played.
fn play(decode: StreamDecodeParams) -> Vec<f32> {
    super::play_decode(decode, SAMPLE_RATE, CHANNELS)
}

/// What a decode from the stream's first frame plays of `span`.
fn from_stream_start(mp3: &[u8], span: SegmentSpan) -> Vec<f32> {
    play(StreamDecodeParams::new(
        vec![SegmentDecodeParams::from_stream_start(
            filled_buffer(mp3),
            span,
            0,
        )],
        false,
        0,
        0,
    ))
}

/// Seek `segment` of `mp3`, with `points` stored for it, `offset` samples in
/// through the playback service's seek mapping, the segment ending at `end`
/// so the decode stops there. Returns the offsets the demuxer read and what
/// played.
fn scrub(
    mp3: &[u8],
    segment: SegmentSpan,
    points: &[ScrubPoint],
    offset: u64,
    end: Option<u64>,
) -> (Vec<u64>, Vec<f32>) {
    let buffer = filled_buffer(mp3);
    let mut prepared = test_prepared_track("track", buffer.clone());
    prepared.content_type = ContentType::Mp3;
    prepared.sample_rate = SAMPLE_RATE;
    prepared.segments[0].span = SegmentSpan {
        end_sample: end,
        ..segment
    };
    prepared.segments[0].scrub_points = ScrubPoints::new(points.to_vec());
    let played = play(prepared.decode_params(offset, false));
    (buffer.read_log(), played)
}

/// Seek to each of `targets` in `mp3` with `points` stored, and check it
/// plays what `reference`, the decode of the whole file from its start, plays
/// from there: a second of it, or to the stream's end from a target within
/// two seconds of it. Returns the targets whose samples differ.
fn differing_targets(
    mp3: &[u8],
    points: &[ScrubPoint],
    reference: &[f32],
    targets: &[u64],
) -> Vec<u64> {
    let channels = CHANNELS as usize;
    let total = (reference.len() / channels) as u64;
    let second = u64::from(SAMPLE_RATE);
    targets
        .iter()
        .copied()
        .filter(|&target| {
            let end = (target + 2 * second < total).then_some(target + second);
            let (_, played) = scrub(mp3, SegmentSpan::whole_file(), points, target, end);
            let stop = end.unwrap_or(total) as usize;
            played != reference[target as usize * channels..stop * channels]
        })
        .collect()
}

/// At each point, at the first sample it plays exactly and the samples on
/// either side of it, at the point's own frame, at fixed places through the
/// file, and at its last samples.
fn targets(points: &[ScrubPoint], total: u64) -> Vec<u64> {
    let second = u64::from(SAMPLE_RATE);
    let mut targets = vec![
        0,
        1,
        4_000,
        10 * second,
        10 * second + 1,
        20 * second - 1,
        25 * second + second / 2,
        40 * second + 12_345,
        59 * second + second / 2,
        total - 20_000,
        total - 1,
    ];
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

#[test]
fn a_scrub_in_an_mp3_plays_the_streams_own_samples_from_its_target() {
    bae_audio::init();
    for (name, mp3) in mp3s() {
        let whole = SegmentSpan::whole_file();
        let points = import_scrub_points(&mp3, whole, &ContentType::Mp3);
        let reference = from_stream_start(&mp3, whole);
        let total = (reference.len() / CHANNELS as usize) as u64;

        // A point for the frames 10, 20, ... 50 seconds in; the first 10
        // seconds decode from the stream's start.
        let ten_seconds = 10 * u64::from(SAMPLE_RATE);
        assert_eq!(points.len(), 5, "{name}: {points:?}");
        for (index, point) in points.iter().enumerate() {
            let served = (index as u64 + 1) * ten_seconds;
            assert!(
                point.sample < point.exact_from && point.exact_from <= served + 1_152,
                "{name}: the point for {served} is {point:?}"
            );
        }

        // A scrub to the first sample a point plays exactly reads the
        // file's first AVIO buffer (the probe), then from the point on:
        // nothing between.
        let avio = bae_audio::AVIO_BUFFER_SIZE as u64;
        for point in &points {
            let end = Some(point.exact_from + u64::from(SAMPLE_RATE));
            let (reads, _) = scrub(&mp3, whole, &points, point.exact_from, end);
            assert!(
                reads.iter().all(|&at| at < avio || at + avio > point.byte),
                "{name}: reads {reads:?} leave the head and the bytes from {point:?}"
            );
        }

        let differing = differing_targets(&mp3, &points, &reference, &targets(&points, total));
        assert!(
            differing.is_empty(),
            "{name}: scrubs to {differing:?} play other samples than the stream's own, points {points:?}"
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
    let mp3 = cbr_mp3(&signal(), 128);
    let whole = SegmentSpan::whole_file();
    let points = import_scrub_points(&mp3, whole, &ContentType::Mp3);
    let reference = from_stream_start(&mp3, whole);

    let mut packets = Packets(Vec::new());
    bae_audio::decode_audio_to_sink(
        filled_buffer(&mp3),
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
