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

    /// Every audio variant's `as_str` MIME must round-trip through `from_mime`.
    /// If this breaks, stored DB rows read back as `Other(...)` and lose the codec.
    #[test]
    fn audio_mime_round_trip() {
        for ct in [
            ContentType::Flac,
            ContentType::Mp3,
            ContentType::Ape,
            ContentType::Alac,
            ContentType::Aac,
            ContentType::Pcm,
            ContentType::Opus,
            ContentType::Vorbis,
            ContentType::WavPack,
            ContentType::Dsd,
        ] {
            assert_eq!(
                ContentType::from_mime(ct.as_str()),
                ct,
                "round trip failed for {:?}",
                ct
            );
        }
    }

    #[test]
    fn image_and_other_mime_round_trip() {
        for ct in [
            ContentType::Jpeg,
            ContentType::Png,
            ContentType::Gif,
            ContentType::Webp,
            ContentType::Bmp,
            ContentType::Svg,
            ContentType::PlainText,
            ContentType::Pdf,
            ContentType::OctetStream,
        ] {
            assert_eq!(
                ContentType::from_mime(ct.as_str()),
                ct,
                "round trip failed for {:?}",
                ct
            );
        }
    }

    #[test]
    fn ape_alternative_mime() {
        // "audio/ape" is an informal variant seen in the wild; "audio/x-ape"
        // is what we emit. Both must map to `Ape`.
        assert_eq!(ContentType::from_mime("audio/ape"), ContentType::Ape);
        assert_eq!(ContentType::from_mime("audio/x-ape"), ContentType::Ape);
    }

    #[test]
    fn new_audio_mimes_are_stable() {
        assert_eq!(ContentType::Pcm.as_str(), "audio/pcm");
        assert_eq!(ContentType::Opus.as_str(), "audio/opus");
        assert_eq!(ContentType::Vorbis.as_str(), "audio/vorbis");
        assert_eq!(ContentType::WavPack.as_str(), "audio/wavpack");
        assert_eq!(ContentType::Dsd.as_str(), "audio/dsd");
    }

    #[test]
    fn unknown_mime_lands_in_other() {
        assert_eq!(
            ContentType::from_mime("audio/ogg"),
            ContentType::Other("audio/ogg".to_string())
        );
        assert_eq!(
            ContentType::from_mime("audio/mp4"),
            ContentType::Other("audio/mp4".to_string())
        );
        assert_eq!(
            ContentType::from_mime("video/mp4"),
            ContentType::Other("video/mp4".to_string())
        );
    }

    #[test]
    fn is_audio_membership() {
        assert!(ContentType::Flac.is_audio());
        assert!(ContentType::Mp3.is_audio());
        assert!(ContentType::Ape.is_audio());
        assert!(ContentType::Alac.is_audio());
        assert!(ContentType::Aac.is_audio());
        assert!(ContentType::Pcm.is_audio());
        assert!(ContentType::Opus.is_audio());
        assert!(ContentType::Vorbis.is_audio());
        assert!(ContentType::WavPack.is_audio());
        assert!(ContentType::Dsd.is_audio());

        assert!(!ContentType::Jpeg.is_audio());
        assert!(!ContentType::PlainText.is_audio());
        assert!(!ContentType::Pdf.is_audio());
        assert!(!ContentType::OctetStream.is_audio());

        // Forward-compat escape hatch: any `Other("audio/...")` is still audio.
        assert!(ContentType::Other("audio/opus".to_string()).is_audio());
        assert!(!ContentType::Other("video/mp4".to_string()).is_audio());
    }

    #[test]
    fn is_image_membership() {
        assert!(ContentType::Jpeg.is_image());
        assert!(ContentType::Png.is_image());
        assert!(ContentType::Gif.is_image());
        assert!(ContentType::Webp.is_image());
        assert!(ContentType::Bmp.is_image());
        assert!(ContentType::Svg.is_image());

        assert!(!ContentType::Flac.is_image());
        assert!(!ContentType::PlainText.is_image());

        assert!(ContentType::Other("image/heic".to_string()).is_image());
        assert!(!ContentType::Other("audio/flac".to_string()).is_image());
    }

    #[test]
    fn display_name() {
        assert_eq!(ContentType::Flac.display_name(), "FLAC");
        assert_eq!(ContentType::Mp3.display_name(), "MP3");
        assert_eq!(ContentType::Ape.display_name(), "APE");
        assert_eq!(ContentType::Alac.display_name(), "ALAC");
        assert_eq!(ContentType::Aac.display_name(), "AAC");
        assert_eq!(ContentType::Pcm.display_name(), "PCM");
        assert_eq!(ContentType::Opus.display_name(), "Opus");
        assert_eq!(ContentType::Vorbis.display_name(), "Vorbis");
        assert_eq!(ContentType::WavPack.display_name(), "WavPack");
        assert_eq!(ContentType::Dsd.display_name(), "DSD");
        assert_eq!(ContentType::Jpeg.display_name(), "JPEG");
    }

    #[test]
    fn file_extension() {
        assert_eq!(ContentType::Flac.file_extension(), "flac");
        assert_eq!(ContentType::Mp3.file_extension(), "mp3");
        assert_eq!(ContentType::Ape.file_extension(), "ape");
        assert_eq!(ContentType::Alac.file_extension(), "m4a");
        assert_eq!(ContentType::Aac.file_extension(), "m4a");
        assert_eq!(ContentType::Pcm.file_extension(), "wav");
        assert_eq!(ContentType::Opus.file_extension(), "opus");
        assert_eq!(ContentType::Vorbis.file_extension(), "ogg");
        assert_eq!(ContentType::WavPack.file_extension(), "wv");
        assert_eq!(ContentType::Dsd.file_extension(), "dsf");
        assert_eq!(ContentType::Jpeg.file_extension(), "jpg");
    }
}
