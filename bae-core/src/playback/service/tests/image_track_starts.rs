//! A track of a single-file image starts where import places it, at the byte
//! a seek to its first sample lands on, whatever the image's codec: playback jumps by byte only to a frame that states its own
//! sample, and seeks by sample to any other.

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
fn image_track2(dir: &std::path::Path, name: &str, format: EncodeFormat) -> (Vec<u8>, SegmentSpan) {
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
    let buffer = || {
        let buffer = create_sparse_buffer(bytes.len() as u64);
        buffer.append_at(0, &bytes);
        buffer
    };

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
