//! Extension-derived classification used before a file is probed.
//!
//! An extension suggests what a file is but doesn't prove it, so the audio
//! variants here name containers and likely codecs (`.flac`, `.mp3`) or a
//! container whose codec is unknown until probed (`.m4a` → `Mp4Container`). The
//! codec-confirmed type is `ContentType`, produced only by probing bytes.
//!
//! Use this for scan-time filtering (is this worth probing?). Never store it.

use crate::content_type::ContentType;
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
            "txt" | "cue" | "log" | "accurip" | "m3u" | "m3u8" => Self::PlainText,
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

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Jpeg | Self::Png | Self::Gif | Self::Webp | Self::Bmp | Self::Svg)
    }

    pub fn is_raster_image(&self) -> bool {
        self.is_image() && !matches!(self, Self::Svg)
    }

    /// Whether this extension suggests a format accepted for cover artwork.
    pub fn is_supported_cover(&self) -> bool {
        self.image_content_type().is_some_and(|content_type| content_type.is_supported_cover())
    }

    pub fn path_is_supported_cover(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| Self::from_extension(extension).is_supported_cover())
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
#[path = "content_type_hint_tests.rs"]
mod tests;
