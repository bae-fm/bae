//! Where a remote renderer fetches this library's media.
//!
//! A fetch-a-URL device plays a track by downloading it over HTTP, so playing to
//! one needs a URL per track rather than a decoded stream.
//! [`RendererMediaSource`] is what the device reaches this library through: the
//! audio-URL minter, the cover-art-URL minter, and the renderer's
//! [flavor](super::RendererFlavor), which decides what it is served. It is built where the channel is (by the
//! caller, over its own HTTP source, which keeps bae-core free of a dependency
//! on bae-subsonic) and handed to the playback service, which serves a fresh
//! track through it every time the queue advances. Holding the gate together
//! with the minters is what keeps a URL's format and the MIME type declared for
//! it from disagreeing.

use std::sync::Arc;

use super::{RendererFlavor, RendererStreamFormat};
use crate::album_detail::ImageRef;
use crate::config::CastTranscodeFormat;
use crate::library::ResolvedTrackAudio;

/// Mints the HTTP URL the renderer fetches a track's audio from, given the track
/// id and the stream format to serve it in; the error is a human-readable
/// reason. The format is passed in — not re-derived here — because the service
/// has already resolved the track's content type and this closure runs
/// synchronously on the service thread, where an async lookup can't.
pub type MediaUrlProvider =
    Arc<dyn Fn(&str, RendererStreamFormat) -> Result<String, String> + Send + Sync>;

/// Mints the HTTP URL for a cover. The URL names the cover's version, so a
/// device that cached the art a replaced cover had fetches the new art.
pub type CoverUrlProvider = Arc<dyn Fn(&ImageRef) -> String + Send + Sync>;

/// Everything a remote renderer needs to reach this library's media. See the
/// [module docs](self).
pub struct RendererMediaSource {
    stream_url: MediaUrlProvider,
    cover_url: CoverUrlProvider,
    /// Chosen where the channel is built, so the service serves each track
    /// through the right flavor as the queue advances without knowing it.
    flavor: RendererFlavor,
}

impl RendererMediaSource {
    pub fn new(
        stream_url: MediaUrlProvider,
        cover_url: CoverUrlProvider,
        flavor: RendererFlavor,
    ) -> Self {
        Self {
            stream_url,
            cover_url,
            flavor,
        }
    }

    /// Where the device fetches one track's stream and its `cover` from. The
    /// served bytes hold the track's stream from its first sample, so the
    /// device's position is a position in that stream: a track that is one
    /// stored file, whole, is served as that file when the flavor decodes its
    /// codec; any other — a window of a CUE image, a pregap read from another
    /// file or generated as silence — is converted from its own segments, to
    /// `transcode` where the flavor takes it. The served format decides both
    /// the URL and the MIME type declared for it, so the two can't disagree.
    pub fn serve_track(
        &self,
        audio: &ResolvedTrackAudio,
        transcode: CastTranscodeFormat,
        cover: Option<&ImageRef>,
    ) -> Result<ServedTrack, String> {
        let format = self.flavor.stream_format(audio, transcode);
        Ok(ServedTrack {
            url: (self.stream_url)(&audio.track_id, format)?,
            content_type: format.content_type_str(&audio.content_type),
            cover_url: cover.map(|cover| (self.cover_url)(cover)),
        })
    }
}

/// One track as the device sees it: the audio URL, the MIME type of the bytes
/// that URL serves, and the cover art to show while it plays.
pub struct ServedTrack {
    pub url: String,
    pub content_type: String,
    pub cover_url: Option<String>,
}
