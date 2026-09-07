use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Probe-verified content type for files stored in the library, stored as a MIME
/// string in the database.
///
/// Every audio variant names a codec we actually decode. Constructed only by
/// [`crate::audio_codec::probe_audio_from_path`] (from `AVCodecID`) or by
/// [`ContentType::from_mime`] (reading a stored MIME back). An extension-based
/// guess goes through [`crate::util::content_type_hint::ContentTypeHint`] and
/// never produces a `ContentType`.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentType {
    // Audio
    Flac,
    Mp3,
    Ape,
    Alac,
    Aac,
    Pcm,
    Opus,
    Vorbis,
    WavPack,
    Dsd,
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
    OctetStream,
    Other(String),
}

impl ContentType {
    /// MIME type string (e.g., "audio/flac", "image/jpeg").
    ///
    /// For ALAC this returns the private MIME `"audio/alac"`; IANA's
    /// `"audio/mp4"` is ambiguous (it covers AAC-in-MP4 too) and can't
    /// round-trip both variants through [`Self::from_mime`]. If a future HTTP
    /// surface needs IANA-correct MIMEs, add a separate `http_mime()`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Flac => "audio/flac",
            Self::Mp3 => "audio/mpeg",
            Self::Ape => "audio/x-ape",
            Self::Alac => "audio/alac",
            Self::Aac => "audio/aac",
            Self::Pcm => "audio/pcm",
            Self::Opus => "audio/opus",
            Self::Vorbis => "audio/vorbis",
            Self::WavPack => "audio/wavpack",
            Self::Dsd => "audio/dsd",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Svg => "image/svg+xml",
            Self::PlainText => "text/plain",
            Self::Pdf => "application/pdf",
            Self::OctetStream => "application/octet-stream",
            Self::Other(s) => s,
        }
    }

    /// Parse from a MIME type string (as stored in the database).
    ///
    /// Every audio variant's [`Self::as_str`] value has a matching arm here —
    /// otherwise a stored value would read back as `Other(...)` and lose its
    /// codec.
    pub fn from_mime(s: &str) -> Self {
        match s {
            "audio/flac" => Self::Flac,
            "audio/mpeg" => Self::Mp3,
            "audio/x-ape" | "audio/ape" => Self::Ape,
            "audio/alac" => Self::Alac,
            "audio/aac" => Self::Aac,
            "audio/pcm" => Self::Pcm,
            "audio/opus" => Self::Opus,
            "audio/vorbis" => Self::Vorbis,
            "audio/wavpack" => Self::WavPack,
            "audio/dsd" => Self::Dsd,
            "image/jpeg" => Self::Jpeg,
            "image/png" => Self::Png,
            "image/gif" => Self::Gif,
            "image/webp" => Self::Webp,
            "image/bmp" => Self::Bmp,
            "image/svg+xml" => Self::Svg,
            "text/plain" => Self::PlainText,
            "application/pdf" => Self::Pdf,
            "application/octet-stream" => Self::OctetStream,
            other => Self::Other(other.to_string()),
        }
    }

    /// File extension for this content type (e.g., "flac", "mp3").
    pub fn file_extension(&self) -> &str {
        match self {
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ape => "ape",
            Self::Alac => "m4a",
            Self::Aac => "m4a",
            Self::Pcm => "wav",
            Self::Opus => "opus",
            Self::Vorbis => "ogg",
            Self::WavPack => "wv",
            Self::Dsd => "dsf",
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Svg => "svg",
            Self::PlainText => "txt",
            Self::Pdf => "pdf",
            Self::OctetStream => "bin",
            Self::Other(_) => "bin",
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::Flac
                | Self::Mp3
                | Self::Ape
                | Self::Alac
                | Self::Aac
                | Self::Pcm
                | Self::Opus
                | Self::Vorbis
                | Self::WavPack
                | Self::Dsd
        ) || matches!(self, Self::Other(s) if s.starts_with("audio/"))
    }

    /// Whether bae has a concrete decoder-backed audio variant for this type.
    /// An arbitrary stored `audio/*` MIME remains audio for display and
    /// diagnostics, but it is not an importable source.
    pub fn is_supported_audio(&self) -> bool {
        matches!(
            self,
            Self::Flac
                | Self::Mp3
                | Self::Ape
                | Self::Alac
                | Self::Aac
                | Self::Pcm
                | Self::Opus
                | Self::Vorbis
                | Self::WavPack
                | Self::Dsd
        )
    }

    /// Whether this codec preserves a source sample width that can be shown as
    /// bit depth. Perceptual codecs may report a coded-word width, but that is
    /// not source sample bit depth.
    pub(crate) fn is_lossless_audio(&self) -> bool {
        matches!(
            self,
            Self::Flac | Self::Ape | Self::Alac | Self::Pcm | Self::WavPack | Self::Dsd
        )
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Gif | Self::Webp | Self::Bmp | Self::Svg
        ) || matches!(self, Self::Other(s) if s.starts_with("image/"))
    }

    /// Short human-readable name for UI display (e.g., "FLAC", "JPEG").
    pub fn display_name(&self) -> &str {
        match self {
            Self::Flac => "FLAC",
            Self::Mp3 => "MP3",
            Self::Ape => "APE",
            Self::Alac => "ALAC",
            Self::Aac => "AAC",
            Self::Pcm => "PCM",
            Self::Opus => "Opus",
            Self::Vorbis => "Vorbis",
            Self::WavPack => "WavPack",
            Self::Dsd => "DSD",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Gif => "GIF",
            Self::Webp => "WebP",
            Self::Bmp => "BMP",
            Self::Svg => "SVG",
            Self::PlainText => "Text",
            Self::Pdf => "PDF",
            Self::OctetStream => "Binary",
            Self::Other(s) => s,
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ContentType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ContentType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(ContentType::from_mime(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ContentType as C;

    /// The canonical variant/string mapping. Every variant's `as_str` MIME must
    /// round-trip through `from_mime` — if that breaks, stored DB rows read back
    /// as `Other(...)` and lose the codec.
    #[test]
    fn mime_display_name_and_extension() {
        for (ct, mime, display_name, extension) in [
            (C::Flac, "audio/flac", "FLAC", "flac"),
            (C::Mp3, "audio/mpeg", "MP3", "mp3"),
            (C::Ape, "audio/x-ape", "APE", "ape"),
            (C::Alac, "audio/alac", "ALAC", "m4a"),
            (C::Aac, "audio/aac", "AAC", "m4a"),
            (C::Pcm, "audio/pcm", "PCM", "wav"),
            (C::Opus, "audio/opus", "Opus", "opus"),
            (C::Vorbis, "audio/vorbis", "Vorbis", "ogg"),
            (C::WavPack, "audio/wavpack", "WavPack", "wv"),
            (C::Dsd, "audio/dsd", "DSD", "dsf"),
            (C::Jpeg, "image/jpeg", "JPEG", "jpg"),
            (C::Png, "image/png", "PNG", "png"),
            (C::Gif, "image/gif", "GIF", "gif"),
            (C::Webp, "image/webp", "WebP", "webp"),
            (C::Bmp, "image/bmp", "BMP", "bmp"),
            (C::Svg, "image/svg+xml", "SVG", "svg"),
            (C::PlainText, "text/plain", "Text", "txt"),
            (C::Pdf, "application/pdf", "PDF", "pdf"),
            (C::OctetStream, "application/octet-stream", "Binary", "bin"),
        ] {
            assert_eq!(ct.as_str(), mime, "{ct:?}");
            assert_eq!(C::from_mime(mime), ct, "round trip failed for {ct:?}");
            assert_eq!(ct.display_name(), display_name, "{ct:?}");
            assert_eq!(ct.file_extension(), extension, "{ct:?}");
        }
    }

    #[test]
    fn ape_alternative_mime() {
        // "audio/ape" is an informal variant seen in the wild; "audio/x-ape"
        // is what we emit. Both must map to `Ape`.
        assert_eq!(C::from_mime("audio/ape"), C::Ape);
        assert_eq!(C::from_mime("audio/x-ape"), C::Ape);
    }

    #[test]
    fn unknown_mime_lands_in_other() {
        for mime in ["audio/ogg", "audio/mp4", "video/mp4"] {
            assert_eq!(C::from_mime(mime), C::Other(mime.to_string()));
        }
    }

    #[test]
    fn is_audio_membership() {
        for ct in [
            C::Flac,
            C::Mp3,
            C::Ape,
            C::Alac,
            C::Aac,
            C::Pcm,
            C::Opus,
            C::Vorbis,
            C::WavPack,
            C::Dsd,
            // Forward-compat escape hatch: any `Other("audio/...")` is still audio.
            C::Other("audio/opus".to_string()),
        ] {
            assert!(ct.is_audio(), "{ct:?}");
        }
        for ct in [
            C::Jpeg,
            C::PlainText,
            C::Pdf,
            C::OctetStream,
            C::Other("video/mp4".to_string()),
        ] {
            assert!(!ct.is_audio(), "{ct:?}");
        }
    }

    #[test]
    fn is_image_membership() {
        for ct in [
            C::Jpeg,
            C::Png,
            C::Gif,
            C::Webp,
            C::Bmp,
            C::Svg,
            C::Other("image/heic".to_string()),
        ] {
            assert!(ct.is_image(), "{ct:?}");
        }
        for ct in [C::Flac, C::PlainText, C::Other("audio/flac".to_string())] {
            assert!(!ct.is_image(), "{ct:?}");
        }
    }

    #[test]
    fn lossless_audio_membership() {
        for ct in [C::Flac, C::Ape, C::Alac, C::Pcm, C::WavPack, C::Dsd] {
            assert!(ct.is_lossless_audio(), "{ct:?}");
        }
        for ct in [C::Mp3, C::Aac, C::Opus, C::Vorbis] {
            assert!(!ct.is_lossless_audio(), "{ct:?}");
        }
    }
}
