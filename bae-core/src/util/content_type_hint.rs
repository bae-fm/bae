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

    #[test]
    fn from_extension_audio() {
        assert_eq!(
            ContentTypeHint::from_extension("flac"),
            ContentTypeHint::Flac
        );
        assert_eq!(
            ContentTypeHint::from_extension("FLAC"),
            ContentTypeHint::Flac
        );
        assert_eq!(ContentTypeHint::from_extension("mp3"), ContentTypeHint::Mp3);
        assert_eq!(ContentTypeHint::from_extension("MP3"), ContentTypeHint::Mp3);
        assert_eq!(ContentTypeHint::from_extension("ape"), ContentTypeHint::Ape);
        assert_eq!(ContentTypeHint::from_extension("APE"), ContentTypeHint::Ape);
        assert_eq!(
            ContentTypeHint::from_extension("m4a"),
            ContentTypeHint::Mp4Container
        );
        assert_eq!(
            ContentTypeHint::from_extension("M4A"),
            ContentTypeHint::Mp4Container
        );
        assert_eq!(
            ContentTypeHint::from_extension("wav"),
            ContentTypeHint::WavContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("aif"),
            ContentTypeHint::AiffContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("aiff"),
            ContentTypeHint::AiffContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("aifc"),
            ContentTypeHint::AiffContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("ogg"),
            ContentTypeHint::OggContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("oga"),
            ContentTypeHint::OggContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("opus"),
            ContentTypeHint::OpusContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("wv"),
            ContentTypeHint::WavPack
        );
        assert_eq!(
            ContentTypeHint::from_extension("dsf"),
            ContentTypeHint::DsdContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("dff"),
            ContentTypeHint::DsdContainer
        );
    }

    #[test]
    fn from_extension_image() {
        assert_eq!(
            ContentTypeHint::from_extension("jpg"),
            ContentTypeHint::Jpeg
        );
        assert_eq!(
            ContentTypeHint::from_extension("jpeg"),
            ContentTypeHint::Jpeg
        );
        assert_eq!(
            ContentTypeHint::from_extension("JPG"),
            ContentTypeHint::Jpeg
        );
        assert_eq!(ContentTypeHint::from_extension("png"), ContentTypeHint::Png);
        assert_eq!(ContentTypeHint::from_extension("gif"), ContentTypeHint::Gif);
        assert_eq!(
            ContentTypeHint::from_extension("webp"),
            ContentTypeHint::Webp
        );
        assert_eq!(ContentTypeHint::from_extension("bmp"), ContentTypeHint::Bmp);
        assert_eq!(ContentTypeHint::from_extension("svg"), ContentTypeHint::Svg);
    }

    #[test]
    fn from_extension_text() {
        assert_eq!(
            ContentTypeHint::from_extension("txt"),
            ContentTypeHint::PlainText
        );
        assert_eq!(
            ContentTypeHint::from_extension("cue"),
            ContentTypeHint::PlainText
        );
        assert_eq!(
            ContentTypeHint::from_extension("log"),
            ContentTypeHint::PlainText
        );
        assert_eq!(
            ContentTypeHint::from_extension("m3u"),
            ContentTypeHint::PlainText
        );
        assert_eq!(
            ContentTypeHint::from_extension("m3u8"),
            ContentTypeHint::PlainText
        );
    }

    #[test]
    fn from_extension_pdf() {
        assert_eq!(ContentTypeHint::from_extension("pdf"), ContentTypeHint::Pdf);
        assert_eq!(ContentTypeHint::from_extension("PDF"), ContentTypeHint::Pdf);
    }

    #[test]
    fn from_extension_unknown_preserves_lowercased_ext() {
        assert_eq!(
            ContentTypeHint::from_extension("wav"),
            ContentTypeHint::WavContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("aac"),
            ContentTypeHint::Unknown("aac".to_string())
        );
        assert_eq!(
            ContentTypeHint::from_extension("aiff"),
            ContentTypeHint::AiffContainer
        );
        assert_eq!(
            ContentTypeHint::from_extension("xyz"),
            ContentTypeHint::Unknown("xyz".to_string())
        );
        // Uppercase in, lowercase preserved.
        assert_eq!(
            ContentTypeHint::from_extension("XYZ"),
            ContentTypeHint::Unknown("xyz".to_string())
        );
    }

    #[test]
    fn is_audio_membership() {
        assert!(ContentTypeHint::Flac.is_audio());
        assert!(ContentTypeHint::Mp3.is_audio());
        assert!(ContentTypeHint::Ape.is_audio());
        assert!(ContentTypeHint::Mp4Container.is_audio());
        assert!(ContentTypeHint::WavContainer.is_audio());
        assert!(ContentTypeHint::AiffContainer.is_audio());
        assert!(ContentTypeHint::OggContainer.is_audio());
        assert!(ContentTypeHint::OpusContainer.is_audio());
        assert!(ContentTypeHint::WavPack.is_audio());
        assert!(ContentTypeHint::DsdContainer.is_audio());

        assert!(!ContentTypeHint::Jpeg.is_audio());
        assert!(!ContentTypeHint::Png.is_audio());
        assert!(!ContentTypeHint::Gif.is_audio());
        assert!(!ContentTypeHint::Webp.is_audio());
        assert!(!ContentTypeHint::Bmp.is_audio());
        assert!(!ContentTypeHint::Svg.is_audio());
        assert!(!ContentTypeHint::PlainText.is_audio());
        assert!(!ContentTypeHint::Pdf.is_audio());
        assert!(!ContentTypeHint::Unknown("wma".to_string()).is_audio());
    }

    #[test]
    fn is_image_membership() {
        assert!(ContentTypeHint::Jpeg.is_image());
        assert!(ContentTypeHint::Png.is_image());
        assert!(ContentTypeHint::Gif.is_image());
        assert!(ContentTypeHint::Webp.is_image());
        assert!(ContentTypeHint::Bmp.is_image());
        assert!(ContentTypeHint::Svg.is_image());

        assert!(!ContentTypeHint::Flac.is_image());
        assert!(!ContentTypeHint::Mp3.is_image());
        assert!(!ContentTypeHint::Ape.is_image());
        assert!(!ContentTypeHint::Mp4Container.is_image());
        assert!(!ContentTypeHint::WavContainer.is_image());
        assert!(!ContentTypeHint::AiffContainer.is_image());
        assert!(!ContentTypeHint::OggContainer.is_image());
        assert!(!ContentTypeHint::OpusContainer.is_image());
        assert!(!ContentTypeHint::WavPack.is_image());
        assert!(!ContentTypeHint::DsdContainer.is_image());
        assert!(!ContentTypeHint::PlainText.is_image());
        assert!(!ContentTypeHint::Pdf.is_image());
        assert!(!ContentTypeHint::Unknown("svg2".to_string()).is_image());
    }

    #[test]
    fn is_raster_image_membership() {
        assert!(ContentTypeHint::Jpeg.is_raster_image());
        assert!(ContentTypeHint::Png.is_raster_image());
        assert!(ContentTypeHint::Gif.is_raster_image());
        assert!(ContentTypeHint::Webp.is_raster_image());
        assert!(ContentTypeHint::Bmp.is_raster_image());

        assert!(!ContentTypeHint::Svg.is_raster_image());
        assert!(!ContentTypeHint::Flac.is_raster_image());
        assert!(!ContentTypeHint::PlainText.is_raster_image());
        assert!(!ContentTypeHint::Pdf.is_raster_image());
        assert!(!ContentTypeHint::Unknown("svg2".to_string()).is_raster_image());
    }

    #[test]
    fn path_is_raster_image_membership() {
        assert!(ContentTypeHint::path_is_raster_image(Path::new(
            "cover.bmp"
        )));
        assert!(!ContentTypeHint::path_is_raster_image(Path::new(
            "cover.svg"
        )));
        assert!(!ContentTypeHint::path_is_raster_image(Path::new("README")));
    }

    #[test]
    fn image_content_type_promotes_image_hints() {
        assert_eq!(
            ContentTypeHint::Jpeg.image_content_type(),
            Some(ContentType::Jpeg)
        );
        assert_eq!(
            ContentTypeHint::Png.image_content_type(),
            Some(ContentType::Png)
        );
        assert_eq!(
            ContentTypeHint::Gif.image_content_type(),
            Some(ContentType::Gif)
        );
        assert_eq!(
            ContentTypeHint::Webp.image_content_type(),
            Some(ContentType::Webp)
        );
        assert_eq!(
            ContentTypeHint::Bmp.image_content_type(),
            Some(ContentType::Bmp)
        );
        assert_eq!(
            ContentTypeHint::Svg.image_content_type(),
            Some(ContentType::Svg)
        );
    }

    #[test]
    fn image_content_type_returns_none_for_non_image_hints() {
        assert_eq!(ContentTypeHint::Flac.image_content_type(), None);
        assert_eq!(ContentTypeHint::Mp3.image_content_type(), None);
        assert_eq!(ContentTypeHint::Mp4Container.image_content_type(), None);
        assert_eq!(ContentTypeHint::WavContainer.image_content_type(), None);
        assert_eq!(ContentTypeHint::AiffContainer.image_content_type(), None);
        assert_eq!(ContentTypeHint::OggContainer.image_content_type(), None);
        assert_eq!(ContentTypeHint::OpusContainer.image_content_type(), None);
        assert_eq!(ContentTypeHint::WavPack.image_content_type(), None);
        assert_eq!(ContentTypeHint::DsdContainer.image_content_type(), None);
        assert_eq!(ContentTypeHint::PlainText.image_content_type(), None);
        assert_eq!(ContentTypeHint::Pdf.image_content_type(), None);
        assert_eq!(
            ContentTypeHint::Unknown("xyz".to_string()).image_content_type(),
            None
        );
    }
}
