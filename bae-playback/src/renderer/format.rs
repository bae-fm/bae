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

use bae_preferences::CastTranscodeFormat;
use bae_records::ResolvedTrackAudio;
use bae_util::audio_container::AudioContainer;
use bae_util::content_type::ContentType;

/// The bitrate an MP3 conversion is encoded at, in kbps. Shared across
/// renderer flavors.
pub const TRANSCODE_BITRATE_KBPS: u32 = 320;

/// How a track is served to the renderer: its original bytes, or converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererStreamFormat {
    /// Serve the stored file's original bytes, in the container they are in
    /// (the renderer decodes them).
    Raw(AudioContainer),
    /// Convert to MP3 at [`TRANSCODE_BITRATE_KBPS`].
    TranscodeMp3,
    /// Convert to uncompressed WAV at the track's stored depth.
    TranscodeWav,
}

impl RendererStreamFormat {
    /// The MIME type of the served bytes: the stored file's container's for a
    /// raw serve, or the conversion's.
    pub fn content_type(&self) -> &'static str {
        match self {
            RendererStreamFormat::Raw(container) => container.http_mime(),
            RendererStreamFormat::TranscodeMp3 => AudioContainer::Mp3.http_mime(),
            RendererStreamFormat::TranscodeWav => AudioContainer::Wav.http_mime(),
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
    /// media format; DLNA's required audio format is uncompressed PCM sent as
    /// `audio/L16`, which bae doesn't serve), so a UPnP renderer is sent MP3
    /// whatever the setting.
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
        if let Some(file) = audio.whole_file().filter(|_| self.decodes(&audio.content_type)) {
            return RendererStreamFormat::Raw(file.container.clone());
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
#[path = "format_tests.rs"]
mod tests;
