//! Extension-derived classification used before a file is probed.
//!
//! An extension suggests what a file is but doesn't prove it, so the audio
//! variants here name containers and likely codecs (`.flac`, `.mp3`) or a
//! container whose codec is unknown until probed (`.m4a` → `Mp4Container`). The
//! codec-confirmed type is `ContentType`, produced only by probing bytes.
//!
//! Use this for scan-time filtering (is this worth probing?). Never store it.

use crate::util::content_type::ContentType;
use std::path::Path;

/// Extension-derived content classification. Container-level for audio;
/// codec confirmation requires a probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentTypeHint {
    // Audio
    Flac,
    Mp3,
    Ape,
    /// `.m4a` — MP4 container. Could wrap ALAC, AAC, or other MP4 audio codecs.
    /// The codec is unknown until the file is probed.
    Mp4Container,
    /// `.wav` — RIFF/WAVE container, probed as PCM or another codec.
    WavContainer,
    /// `.aif`, `.aiff`, `.aifc` — AIFF/AIFF-C container, probed as PCM or another codec.
    AiffContainer,
    /// `.ogg`, `.oga` — Ogg container. Could wrap Vorbis, Opus, or FLAC.
    OggContainer,
    /// `.opus` — Ogg Opus container by convention; FFmpeg still verifies it.
    OpusContainer,
    /// `.wv` — WavPack container.
    WavPack,
    /// `.dsf`, `.dff` — DSD containers.
    DsdContainer,
    // Images
    Jpeg,
    Png,
    Gif,
    Webp,
    Bmp,
    Svg,
    // Text
    PlainText,
    // Other
    Pdf,
    /// Anything unrecognised. The lowercased extension is preserved for
    /// diagnostics (logs, error messages).
    Unknown(String),
}

impl ContentTypeHint {
    /// Classify by file extension. The input is lowercased before matching.
    pub fn from_extension(ext: &str) -> Self {
        let ext = ext.to_lowercase();
        match ext.as_str() {
            "flac" => Self::Flac,
            "mp3" => Self::Mp3,
            "ape" => Self::Ape,
            "m4a" => Self::Mp4Container,
            "wav" => Self::WavContainer,
            "aif" | "aiff" | "aifc" => Self::AiffContainer,
            "ogg" | "oga" => Self::OggContainer,
            "opus" => Self::OpusContainer,
            "wv" => Self::WavPack,
            "dsf" | "dff" => Self::DsdContainer,
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            "bmp" => Self::Bmp,
            "svg" => Self::Svg,
            "txt" | "cue" | "log" | "m3u" | "m3u8" => Self::PlainText,
            "pdf" => Self::Pdf,
            _ => Self::Unknown(ext),
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::Flac
                | Self::Mp3
                | Self::Ape
                | Self::Mp4Container
                | Self::WavContainer
                | Self::AiffContainer
                | Self::OggContainer
                | Self::OpusContainer
                | Self::WavPack
                | Self::DsdContainer
        )
    }

    /// Whether `path`'s extension classifies as audio. Returns `false` for
    /// paths with no extension or non-UTF-8 extensions.
    pub fn path_is_audio(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| Self::from_extension(e).is_audio())
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Gif | Self::Webp | Self::Bmp | Self::Svg
        )
    }

    pub fn is_raster_image(&self) -> bool {
        self.is_image() && !matches!(self, Self::Svg)
    }

    /// Whether `path`'s extension classifies as a raster image. Returns `false`
    /// for paths with no extension, non-UTF-8 extensions, and SVG.
    pub fn path_is_raster_image(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| Self::from_extension(e).is_raster_image())
    }

    /// The `ContentType` an image hint promotes to without a probe — image
    /// extensions predict their format reliably (a `.png` file's bytes are PNG).
    /// `None` for audio (which needs the probe) and for text/PDF/unknown, which
    /// the caller resolves itself.
    pub fn image_content_type(&self) -> Option<ContentType> {
        match self {
            Self::Jpeg => Some(ContentType::Jpeg),
            Self::Png => Some(ContentType::Png),
            Self::Gif => Some(ContentType::Gif),
            Self::Webp => Some(ContentType::Webp),
            Self::Bmp => Some(ContentType::Bmp),
            Self::Svg => Some(ContentType::Svg),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ContentType as C;
    use ContentTypeHint as H;

    #[test]
    fn from_extension_classifies_by_extension() {
        for (ext, expected) in [
            ("flac", H::Flac),
            ("FLAC", H::Flac),
            ("mp3", H::Mp3),
            ("MP3", H::Mp3),
            ("ape", H::Ape),
            ("APE", H::Ape),
            ("m4a", H::Mp4Container),
            ("M4A", H::Mp4Container),
            ("wav", H::WavContainer),
            ("aif", H::AiffContainer),
            ("aiff", H::AiffContainer),
            ("aifc", H::AiffContainer),
            ("ogg", H::OggContainer),
            ("oga", H::OggContainer),
            ("opus", H::OpusContainer),
            ("wv", H::WavPack),
            ("dsf", H::DsdContainer),
            ("dff", H::DsdContainer),
            ("jpg", H::Jpeg),
            ("jpeg", H::Jpeg),
            ("JPG", H::Jpeg),
            ("png", H::Png),
            ("gif", H::Gif),
            ("webp", H::Webp),
            ("bmp", H::Bmp),
            ("svg", H::Svg),
            ("txt", H::PlainText),
            ("cue", H::PlainText),
            ("log", H::PlainText),
            ("m3u", H::PlainText),
            ("m3u8", H::PlainText),
            ("pdf", H::Pdf),
            ("PDF", H::Pdf),
            ("aac", H::Unknown("aac".to_string())),
            ("xyz", H::Unknown("xyz".to_string())),
            // Uppercase in, lowercase preserved.
            ("XYZ", H::Unknown("xyz".to_string())),
        ] {
            assert_eq!(H::from_extension(ext), expected, "{ext}");
        }
    }

    #[test]
    fn is_audio_membership() {
        for hint in [
            H::Flac,
            H::Mp3,
            H::Ape,
            H::Mp4Container,
            H::WavContainer,
            H::AiffContainer,
            H::OggContainer,
            H::OpusContainer,
            H::WavPack,
            H::DsdContainer,
        ] {
            assert!(hint.is_audio(), "{hint:?}");
        }
        for hint in [
            H::Jpeg,
            H::Png,
            H::Gif,
            H::Webp,
            H::Bmp,
            H::Svg,
            H::PlainText,
            H::Pdf,
            H::Unknown("wma".to_string()),
        ] {
            assert!(!hint.is_audio(), "{hint:?}");
        }
    }

    #[test]
    fn is_image_membership() {
        for hint in [H::Jpeg, H::Png, H::Gif, H::Webp, H::Bmp, H::Svg] {
            assert!(hint.is_image(), "{hint:?}");
        }
        for hint in [
            H::Flac,
            H::Mp3,
            H::Ape,
            H::Mp4Container,
            H::WavContainer,
            H::AiffContainer,
            H::OggContainer,
            H::OpusContainer,
            H::WavPack,
            H::DsdContainer,
            H::PlainText,
            H::Pdf,
            H::Unknown("svg2".to_string()),
        ] {
            assert!(!hint.is_image(), "{hint:?}");
        }
    }

    #[test]
    fn is_raster_image_membership() {
        for hint in [H::Jpeg, H::Png, H::Gif, H::Webp, H::Bmp] {
            assert!(hint.is_raster_image(), "{hint:?}");
        }
        for hint in [
            H::Svg,
            H::Flac,
            H::PlainText,
            H::Pdf,
            H::Unknown("svg2".to_string()),
        ] {
            assert!(!hint.is_raster_image(), "{hint:?}");
        }
    }

    #[test]
    fn path_is_raster_image_membership() {
        assert!(H::path_is_raster_image(Path::new("cover.bmp")));
        assert!(!H::path_is_raster_image(Path::new("cover.svg")));
        assert!(!H::path_is_raster_image(Path::new("README")));
    }

    #[test]
    fn image_content_type_promotes_only_image_hints() {
        for (hint, expected) in [
            (H::Jpeg, Some(C::Jpeg)),
            (H::Png, Some(C::Png)),
            (H::Gif, Some(C::Gif)),
            (H::Webp, Some(C::Webp)),
            (H::Bmp, Some(C::Bmp)),
            (H::Svg, Some(C::Svg)),
            (H::Flac, None),
            (H::Mp3, None),
            (H::Mp4Container, None),
            (H::WavContainer, None),
            (H::AiffContainer, None),
            (H::OggContainer, None),
            (H::OpusContainer, None),
            (H::WavPack, None),
            (H::DsdContainer, None),
            (H::PlainText, None),
            (H::Pdf, None),
            (H::Unknown("xyz".to_string()), None),
        ] {
            assert_eq!(hint.image_content_type(), expected, "{hint:?}");
        }
    }
}
