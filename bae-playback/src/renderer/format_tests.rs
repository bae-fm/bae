use super::*;
use bae_records::ResolvedTrackAudioSegment;
use bae_records::{DbAudioSegmentRole, SegmentSpan};

/// A track stored as `content_type` in a file in `container`, as that whole
/// file when `whole_file`, else as a window of a CUE image.
fn track(
    content_type: ContentType,
    container: AudioContainer,
    whole_file: bool,
) -> ResolvedTrackAudio {
    let span = if whole_file {
        SegmentSpan::whole_file()
    } else {
        SegmentSpan {
            start_sample: 441_000,
            end_sample: Some(882_000),
            start_byte: None,
            end_byte: None,
        }
    };
    ResolvedTrackAudio {
        track_id: "track".to_string(),
        release_id: "release".to_string(),
        segments: vec![ResolvedTrackAudioSegment {
            role: DbAudioSegmentRole::Main,
            file_id: "file".to_string(),
            file_size: 1_000_000,
            container,
            span,
            scrub_points: bae_records::ScrubPoints::new(Vec::new()),
        }],
        duration_ms: 10_000,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        bits_per_sample: Some(16),
        content_type,
        track_loudness_lufs: None,
        track_peak_linear: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
    }
}

/// Every (flavor, setting) pair for a track, in the order
/// Cast/MP3, Cast/WAV, UPnP/MP3, UPnP/WAV.
fn served(audio: &ResolvedTrackAudio) -> [RendererStreamFormat; 4] {
    use CastTranscodeFormat::{Mp3, Wav};
    use RendererFlavor::{Cast, Dlna};
    [
        Cast.stream_format(audio, Mp3),
        Cast.stream_format(audio, Wav),
        Dlna.stream_format(audio, Mp3),
        Dlna.stream_format(audio, Wav),
    ]
}

use RendererStreamFormat::{Raw, TranscodeMp3, TranscodeWav};

/// A whole file in a codec both flavors decode goes out as stored, whatever
/// the setting: the setting is only what a conversion produces.
#[test]
fn a_whole_file_both_flavors_decode_is_served_as_stored() {
    for (content_type, container) in [
        (ContentType::Flac, AudioContainer::Flac),
        (ContentType::Mp3, AudioContainer::Mp3),
        (ContentType::Aac, AudioContainer::Mp4),
        (ContentType::Pcm, AudioContainer::Wav),
    ] {
        let raw = Raw(container.clone());
        assert_eq!(
            served(&track(content_type.clone(), container, true)),
            [raw.clone(), raw.clone(), raw.clone(), raw],
            "{content_type:?}"
        );
    }
}

/// A CUE image's window is converted for every flavor: to WAV for Cast when
/// the setting asks for it, and to MP3 for UPnP either way, since a UPnP
/// renderer isn't counted on to play WAV.
#[test]
fn a_window_converts_to_the_setting_where_the_flavor_takes_it() {
    assert_eq!(
        served(&track(ContentType::Flac, AudioContainer::Flac, false)),
        [TranscodeMp3, TranscodeWav, TranscodeMp3, TranscodeMp3]
    );
}

/// A whole file in a codec neither flavor decodes is converted like a
/// window is.
#[test]
fn a_codec_no_flavor_decodes_converts_to_the_setting() {
    for (content_type, container) in [
        (ContentType::Ape, AudioContainer::Ape),
        (ContentType::Alac, AudioContainer::Mp4),
        (ContentType::WavPack, AudioContainer::WavPack),
        (ContentType::Dsd, AudioContainer::Dsf),
    ] {
        assert_eq!(
            served(&track(content_type.clone(), container, true)),
            [TranscodeMp3, TranscodeWav, TranscodeMp3, TranscodeMp3],
            "{content_type:?}"
        );
    }
}

/// Opus is where the flavors' decoders differ: Cast plays the file as
/// stored, a UPnP renderer gets a conversion.
#[test]
fn opus_plays_raw_on_cast_and_converts_for_upnp() {
    let raw = Raw(AudioContainer::Ogg);
    assert_eq!(
        served(&track(ContentType::Opus, AudioContainer::Ogg, true)),
        [raw.clone(), raw, TranscodeMp3, TranscodeMp3]
    );
}

/// A raw serve names the container its bytes are in, not the codec they
/// hold: Opus in Ogg is `audio/ogg`, PCM in WAV `audio/wav`.
#[test]
fn served_content_type_names_the_container() {
    assert_eq!(Raw(AudioContainer::Flac).content_type(), "audio/flac");
    assert_eq!(Raw(AudioContainer::Ogg).content_type(), "audio/ogg");
    assert_eq!(Raw(AudioContainer::Wav).content_type(), "audio/wav");
    assert_eq!(TranscodeMp3.content_type(), "audio/mpeg");
    assert_eq!(TranscodeWav.content_type(), "audio/wav");
}
