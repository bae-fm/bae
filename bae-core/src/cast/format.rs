//! Which stream format a Cast receiver is served for a given source codec.
//!
//! Cast devices decode FLAC, MP3, AAC, Opus, and WAV/PCM natively, so those are
//! served as their original bytes. Anything else (APE, ALAC, WavPack, DSD, …) is
//! transcoded to MP3 on the way out. This gate is the single source both the URL
//! provider (which picks `format=raw` vs `format=mp3`) and the LOAD metadata
//! (which reports the served MIME type) consult, so the URL and the declared
//! content type never disagree.

use crate::util::content_type::ContentType;

/// The bitrate a non-Cast-safe source is transcoded to, in kbps.
pub const CAST_TRANSCODE_BITRATE_KBPS: u32 = 320;

/// How a track is served to the receiver: its original bytes, or transcoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastStreamFormat {
    /// Serve the backing file's original bytes (the receiver decodes them).
    Raw,
    /// Transcode to MP3 at [`CAST_TRANSCODE_BITRATE_KBPS`] (the source codec is
    /// not Cast-decodable).
    TranscodeMp3,
}

impl CastStreamFormat {
    /// The MIME type of the served bytes: the source type for a raw serve, or
    /// `audio/mpeg` for a transcode.
    pub fn content_type_str(self, source: &ContentType) -> String {
        match self {
            CastStreamFormat::Raw => source.as_str().to_string(),
            CastStreamFormat::TranscodeMp3 => "audio/mpeg".to_string(),
        }
    }
}

/// Whether `content_type` is a codec Cast devices decode natively.
fn is_cast_decodable(content_type: &ContentType) -> bool {
    matches!(
        content_type,
        ContentType::Flac
            | ContentType::Mp3
            | ContentType::Aac
            | ContentType::Opus
            | ContentType::Pcm
    )
}

/// The stream format to serve for a source of `content_type`: raw when the
/// receiver can decode it, an MP3 transcode otherwise.
pub fn cast_stream_format(content_type: &ContentType) -> CastStreamFormat {
    if is_cast_decodable(content_type) {
        CastStreamFormat::Raw
    } else {
        CastStreamFormat::TranscodeMp3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_safe_codecs_serve_raw() {
        for ct in [
            ContentType::Flac,
            ContentType::Mp3,
            ContentType::Aac,
            ContentType::Opus,
            ContentType::Pcm,
        ] {
            assert_eq!(cast_stream_format(&ct), CastStreamFormat::Raw, "{ct:?}");
        }
    }

    #[test]
    fn exotic_codecs_transcode_to_mp3() {
        for ct in [
            ContentType::Ape,
            ContentType::Alac,
            ContentType::WavPack,
            ContentType::Dsd,
        ] {
            assert_eq!(
                cast_stream_format(&ct),
                CastStreamFormat::TranscodeMp3,
                "{ct:?}"
            );
        }
    }

    #[test]
    fn served_content_type_follows_the_format() {
        assert_eq!(
            CastStreamFormat::Raw.content_type_str(&ContentType::Flac),
            "audio/flac"
        );
        assert_eq!(
            CastStreamFormat::TranscodeMp3.content_type_str(&ContentType::Ape),
            "audio/mpeg"
        );
    }
}
