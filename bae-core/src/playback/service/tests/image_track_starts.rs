//! A track of a single-file image starts where import placed it, whatever the
//! image's codec: playback jumps by byte only to a frame that states its own
//! sample, and seeks by sample to any other.

use super::*;
use crate::db::SegmentSpan;
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use bae_audio::EncodeFormat;
use bae_util::content_type::ContentType;

/// Start track 2 of a `format` image as playback does on a natural start,
/// and return what it played and what a decode of the whole image plays from
/// the track's first sample on.
fn track2_and_reference(
    name: &str,
    format: EncodeFormat,
    content_type: ContentType,
) -> (Vec<f32>, Vec<f32>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let (bytes, segments) = crate::test_images::cue_image(temp.path(), name, format);
    let track2 = segments.last().expect("track 2's segment");
    let buffer = || {
        let buffer = create_sparse_buffer(bytes.len() as u64);
        buffer.append_at(0, &bytes);
        buffer
    };

    let mut prepared = test_prepared_track("track-2", buffer());
    prepared.content_type = content_type;
    prepared.segments[0].span = track2.span();
    let played = play_decode(prepared.decode_params(0, false), 44_100, 2);

    let whole = play_decode(
        StreamDecodeParams::new(
            vec![SegmentDecodeParams::new(
                buffer(),
                SegmentSpan::whole_file(),
                0,
            )],
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
        EncodeFormat::PcmWav {
            bits_per_sample: 16,
        },
        ContentType::Pcm,
    );
    assert_eq!(played.len(), reference.len(), "sample count");
    assert!(
        played == reference,
        "the track's samples differ from the image's"
    );
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
    assert!(
        played == reference,
        "the track's samples differ from the image's"
    );
}
