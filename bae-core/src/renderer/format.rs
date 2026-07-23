//! Which stream format a remote renderer is served for a given source codec.
//!
//! A renderer decodes a limited set of codecs natively; a source in one of them
//! is served as its original bytes, anything else is transcoded to MP3 on the way
//! out. The safe set differs by renderer flavor — Cast decodes Opus, most UPnP
//! renderers don't — so each flavor has its own gate ([`cast_stream_format`],
//! [`dlna_stream_format`]) over the shared [`RendererStreamFormat`] decision. The
//! gate is the single source both the URL provider (which picks `format=raw` vs
//! `format=mp3`) and the LOAD metadata (which reports the served MIME type)
//! consult, so the URL and the declared content type never disagree.

use crate::util::content_type::ContentType;

/// The bitrate a non-decodable source is transcoded to, in kbps. Shared across
/// renderer flavors.
pub const TRANSCODE_BITRATE_KBPS: u32 = 320;

/// How a track is served to the renderer: its original bytes, or transcoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererStreamFormat {
    /// Serve the backing file's original bytes (the renderer decodes them).
    Raw,
    /// Transcode to MP3 at [`TRANSCODE_BITRATE_KBPS`] (the source codec is not
    /// decodable by this renderer).
    TranscodeMp3,
}

impl RendererStreamFormat {
    /// The MIME type of the served bytes: the source type for a raw serve, or
    /// `audio/mpeg` for a transcode.
    pub fn content_type_str(self, source: &ContentType) -> String {
        match self {
            RendererStreamFormat::Raw => source.as_str().to_string(),
            RendererStreamFormat::TranscodeMp3 => "audio/mpeg".to_string(),
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

/// Whether `content_type` is a codec UPnP MediaRenderers broadly decode
/// natively. Narrower than Cast's: Opus is widely unsupported by UPnP renderers,
/// so it transcodes even though Cast plays it raw.
fn is_dlna_decodable(content_type: &ContentType) -> bool {
    matches!(
        content_type,
        ContentType::Flac | ContentType::Mp3 | ContentType::Aac | ContentType::Pcm
    )
}

/// The stream format to serve a Cast receiver for a source of `content_type`:
/// raw when the receiver can decode it, an MP3 transcode otherwise.
pub fn cast_stream_format(content_type: &ContentType) -> RendererStreamFormat {
    if is_cast_decodable(content_type) {
        RendererStreamFormat::Raw
    } else {
        RendererStreamFormat::TranscodeMp3
    }
}

/// The stream format to serve a UPnP MediaRenderer for a source of
/// `content_type`: raw when the renderer can decode it, an MP3 transcode
/// otherwise (notably for Opus).
pub fn dlna_stream_format(content_type: &ContentType) -> RendererStreamFormat {
    if is_dlna_decodable(content_type) {
        RendererStreamFormat::Raw
    } else {
        RendererStreamFormat::TranscodeMp3
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
            assert_eq!(cast_stream_format(&ct), RendererStreamFormat::Raw, "{ct:?}");
        }
    }

    #[test]
    fn cast_exotic_codecs_transcode_to_mp3() {
        for ct in [
            ContentType::Ape,
            ContentType::Alac,
            ContentType::WavPack,
            ContentType::Dsd,
        ] {
            assert_eq!(
                cast_stream_format(&ct),
                RendererStreamFormat::TranscodeMp3,
                "{ct:?}"
            );
        }
    }

    #[test]
    fn dlna_safe_codecs_serve_raw() {
        for ct in [
            ContentType::Flac,
            ContentType::Mp3,
            ContentType::Aac,
            ContentType::Pcm,
        ] {
            assert_eq!(dlna_stream_format(&ct), RendererStreamFormat::Raw, "{ct:?}");
        }
    }

    #[test]
    fn dlna_transcodes_opus_that_cast_plays_raw() {
        // The meaningful divergence between the two gates: Cast decodes Opus, so
        // it serves raw; UPnP renderers broadly don't, so the same source
        // transcodes.
        assert_eq!(
            cast_stream_format(&ContentType::Opus),
            RendererStreamFormat::Raw
        );
        assert_eq!(
            dlna_stream_format(&ContentType::Opus),
            RendererStreamFormat::TranscodeMp3
        );
    }

    #[test]
    fn served_content_type_follows_the_format() {
        assert_eq!(
            RendererStreamFormat::Raw.content_type_str(&ContentType::Flac),
            "audio/flac"
        );
        assert_eq!(
            RendererStreamFormat::TranscodeMp3.content_type_str(&ContentType::Ape),
            "audio/mpeg"
        );
    }
}
