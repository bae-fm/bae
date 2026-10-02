//! What stream a remote renderer is served for a track.
//!
//! A renderer decodes a limited set of codecs natively; a track stored as one
//! whole file in one of them is served as its original bytes, and anything else
//! — a window of a CUE image, a track whose pregap lives in another file or is
//! generated, a codec the renderer doesn't decode — is converted on the way out
//! to the format the person picked ([`CastTranscodeFormat`]), when the renderer
//! takes it. The safe sets differ by [`RendererFlavor`]: Cast decodes Opus and
//! plays WAV, UPnP renderers aren't counted on for either. The flavor's
//! [`stream_format`](RendererFlavor::stream_format) is the single source both
//! the URL provider (which picks `format=raw`, `format=mp3` or `format=wav`)
//! and the LOAD metadata (which reports the served MIME type) consult, so the
//! URL and the declared content type never disagree.

use crate::config::CastTranscodeFormat;
use crate::library::ResolvedTrackAudio;
use crate::util::content_type::ContentType;

/// The bitrate an MP3 conversion is encoded at, in kbps. Shared across
/// renderer flavors.
pub const TRANSCODE_BITRATE_KBPS: u32 = 320;

/// How a track is served to the renderer: its original bytes, or converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererStreamFormat {
    /// Serve the stored file's original bytes (the renderer decodes them).
    Raw,
    /// Convert to MP3 at [`TRANSCODE_BITRATE_KBPS`].
    TranscodeMp3,
    /// Convert to uncompressed WAV at the track's stored depth.
    TranscodeWav,
}

impl RendererStreamFormat {
    /// The MIME type of the served bytes: the source type for a raw serve, or
    /// the conversion's.
    pub fn content_type_str(self, source: &ContentType) -> String {
        match self {
            RendererStreamFormat::Raw => source.as_str().to_string(),
            RendererStreamFormat::TranscodeMp3 => "audio/mpeg".to_string(),
            RendererStreamFormat::TranscodeWav => "audio/wav".to_string(),
        }
    }
}

/// The flavors of renderer that fetch a URL, each with what it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererFlavor {
    /// A Google Cast receiver.
    Cast,
    /// A UPnP MediaRenderer.
    Dlna,
}

impl RendererFlavor {
    /// Whether a renderer of this flavor decodes `content_type` natively. UPnP
    /// renderers' set is narrower than Cast's: Opus is widely unsupported by
    /// them, so it converts even though Cast plays it raw.
    fn decodes(self, content_type: &ContentType) -> bool {
        match self {
            RendererFlavor::Cast => matches!(
                content_type,
                ContentType::Flac
                    | ContentType::Mp3
                    | ContentType::Aac
                    | ContentType::Opus
                    | ContentType::Pcm
            ),
            RendererFlavor::Dlna => matches!(
                content_type,
                ContentType::Flac | ContentType::Mp3 | ContentType::Aac | ContentType::Pcm
            ),
        }
    }

    /// Whether a renderer of this flavor plays the WAV bae converts to — a
    /// stream whose RIFF sizes say "unknown", since its length isn't known
    /// until it ends. Cast receivers list WAV among the formats they play.
    /// UPnP renderers aren't required to play WAV at all (it is not a DLNA
    /// media format), so a UPnP renderer is sent MP3 whatever the setting.
    pub fn takes_wav(self) -> bool {
        match self {
            RendererFlavor::Cast => true,
            RendererFlavor::Dlna => false,
        }
    }

    /// The stream to serve a renderer of this flavor for `audio`: the stored
    /// file, when the track is one whole file in a codec the renderer decodes;
    /// otherwise a conversion to `transcode`, or to MP3 where the flavor
    /// doesn't take WAV.
    pub fn stream_format(
        self,
        audio: &ResolvedTrackAudio,
        transcode: CastTranscodeFormat,
    ) -> RendererStreamFormat {
        if audio.whole_file().is_some() && self.decodes(&audio.content_type) {
            return RendererStreamFormat::Raw;
        }
        match transcode {
            CastTranscodeFormat::Wav if self.takes_wav() => RendererStreamFormat::TranscodeWav,
            CastTranscodeFormat::Wav | CastTranscodeFormat::Mp3 => {
                RendererStreamFormat::TranscodeMp3
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DbAudioSegmentRole, SegmentSpan};
    use crate::library::ResolvedTrackAudioSegment;

    /// A track stored as `content_type`, as one whole file when `whole_file`,
    /// else as a window of a CUE image.
    fn track(content_type: ContentType, whole_file: bool) -> ResolvedTrackAudio {
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
                cloud_path: None,
                file_size: 1_000_000,
                span,
            }],
            duration_ms: Some(10_000),
            pregap_ms: None,
            generated_pregap_ms: None,
            pregap_samples: None,
            generated_pregap_samples: None,
            sample_rate: 44_100,
            channels: 2,
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
        for content_type in [
            ContentType::Flac,
            ContentType::Mp3,
            ContentType::Aac,
            ContentType::Pcm,
        ] {
            assert_eq!(
                served(&track(content_type.clone(), true)),
                [Raw, Raw, Raw, Raw],
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
            served(&track(ContentType::Flac, false)),
            [TranscodeMp3, TranscodeWav, TranscodeMp3, TranscodeMp3]
        );
    }

    /// A whole file in a codec neither flavor decodes is converted like a
    /// window is.
    #[test]
    fn a_codec_no_flavor_decodes_converts_to_the_setting() {
        for content_type in [
            ContentType::Ape,
            ContentType::Alac,
            ContentType::WavPack,
            ContentType::Dsd,
        ] {
            assert_eq!(
                served(&track(content_type.clone(), true)),
                [TranscodeMp3, TranscodeWav, TranscodeMp3, TranscodeMp3],
                "{content_type:?}"
            );
        }
    }

    /// Opus is where the flavors' decoders differ: Cast plays the file as
    /// stored, a UPnP renderer gets a conversion.
    #[test]
    fn opus_plays_raw_on_cast_and_converts_for_upnp() {
        assert_eq!(
            served(&track(ContentType::Opus, true)),
            [Raw, Raw, TranscodeMp3, TranscodeMp3]
        );
    }

    #[test]
    fn served_content_type_follows_the_format() {
        assert_eq!(Raw.content_type_str(&ContentType::Flac), "audio/flac");
        assert_eq!(
            TranscodeMp3.content_type_str(&ContentType::Ape),
            "audio/mpeg"
        );
        assert_eq!(
            TranscodeWav.content_type_str(&ContentType::Ape),
            "audio/wav"
        );
    }
}
