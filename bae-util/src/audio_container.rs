use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The container an audio file's bytes are in, as the probe read it from the
/// bytes: what a served file's HTTP `Content-Type` names.
///
/// A codec ([`crate::content_type::ContentType`]) doesn't decide it: PCM sits
/// in WAV or AIFF, FLAC in its own container or in Ogg, AAC in ADTS or MP4,
/// and an extension is only a guess at it
/// ([`crate::content_type_hint::ContentTypeHint`]). Constructed by
/// bae-audio's probe (from the demuxer FFmpeg picked), by the encoder's
/// stream formats, or by [`AudioContainer::from_name`] reading a stored name
/// back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioContainer {
    Flac,
    Mp3,
    Wav,
    Aiff,
    Ogg,
    /// MPEG-4 audio (`.m4a`), holding AAC or ALAC.
    Mp4,
    /// Raw AAC in ADTS frames (`.aac`).
    Adts,
    Ape,
    WavPack,
    Dsf,
    Dsdiff,
    /// A container bae names no type for: the probe's `demuxer:<name>`.
    Other(String),
}

impl AudioContainer {
    /// The name stored for this container.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Ogg => "ogg",
            Self::Mp4 => "mp4",
            Self::Adts => "adts",
            Self::Ape => "ape",
            Self::WavPack => "wavpack",
            Self::Dsf => "dsf",
            Self::Dsdiff => "dsdiff",
            Self::Other(name) => name,
        }
    }

    /// Read a stored name back. Every [`Self::as_str`] value has an arm here,
    /// so a stored container never reads back as another.
    pub fn from_name(name: &str) -> Self {
        match name {
            "flac" => Self::Flac,
            "mp3" => Self::Mp3,
            "wav" => Self::Wav,
            "aiff" => Self::Aiff,
            "ogg" => Self::Ogg,
            "mp4" => Self::Mp4,
            "adts" => Self::Adts,
            "ape" => Self::Ape,
            "wavpack" => Self::WavPack,
            "dsf" => Self::Dsf,
            "dsdiff" => Self::Dsdiff,
            other => Self::Other(other.to_string()),
        }
    }

    /// The MIME type of bytes in this container, as an HTTP `Content-Type`
    /// names them. A container with no registered type takes the name players
    /// know it by; one bae names no type for is plain bytes.
    pub fn http_mime(&self) -> &'static str {
        match self {
            Self::Flac => "audio/flac",
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
            Self::Aiff => "audio/aiff",
            Self::Ogg => "audio/ogg",
            Self::Mp4 => "audio/mp4",
            Self::Adts => "audio/aac",
            Self::Ape => "audio/x-ape",
            Self::WavPack => "audio/x-wavpack",
            Self::Dsf => "audio/x-dsf",
            Self::Dsdiff => "audio/x-dff",
            Self::Other(_) => "application/octet-stream",
        }
    }
}

impl Serialize for AudioContainer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AudioContainer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_name(&String::deserialize(deserializer)?))
    }
}

#[cfg(test)]
#[path = "audio_container_tests.rs"]
mod tests;
