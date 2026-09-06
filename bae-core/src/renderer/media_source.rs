//! Where a remote renderer fetches this library's media.
//!
//! A fetch-a-URL device plays a track by downloading it over HTTP, so playing to
//! one needs a URL per track rather than a decoded stream.
//! [`RendererMediaSource`] is what the device reaches this library through: the
//! audio-URL minter, the cover-art-URL minter, and the flavor's
//! [format](mod@super::format) gate. It is built where the channel is (by the
//! caller, over its own HTTP source, which keeps bae-core free of a dependency
//! on bae-subsonic) and handed to the playback service, which serves a fresh
//! track through it every time the queue advances. Holding the gate together
//! with the minters is what keeps a URL's format and the MIME type declared for
//! it from disagreeing.

use std::sync::Arc;

use super::RendererStreamFormat;
use crate::util::content_type::ContentType;

/// Mints the HTTP URL the renderer fetches a track's audio from, given the track
/// id and the stream format to serve it in; the error is a human-readable
/// reason. The format is passed in — not re-derived here — because the service
/// has already resolved the track's content type and this closure runs
/// synchronously on the service thread, where an async lookup can't.
pub type MediaUrlProvider =
    Arc<dyn Fn(&str, RendererStreamFormat) -> Result<String, String> + Send + Sync>;

/// Mints the HTTP URL for a track's cover art, given the track id, or `None`
/// when the track has no cover.
pub type CoverUrlProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Picks the served stream format for a source codec. The flavor-specific gate
/// ([`cast_stream_format`](super::cast_stream_format) or
/// [`dlna_stream_format`](super::dlna_stream_format)) is chosen where the channel
/// is built, so the service reissues each track through the right safe-set as the
/// queue advances without knowing the flavor.
pub type StreamFormatFn = fn(&ContentType) -> RendererStreamFormat;

/// Everything a remote renderer needs to reach this library's media. See the
/// [module docs](self).
pub struct RendererMediaSource {
    stream_url: MediaUrlProvider,
    cover_url: CoverUrlProvider,
    stream_format: StreamFormatFn,
}

impl RendererMediaSource {
    pub fn new(
        stream_url: MediaUrlProvider,
        cover_url: CoverUrlProvider,
        stream_format: StreamFormatFn,
    ) -> Self {
        Self {
            stream_url,
            cover_url,
            stream_format,
        }
    }

    /// Where the device fetches one track from. The format gate runs here, on the
    /// track's now-known source codec, and the served format decides both the URL
    /// and the MIME type declared for it, so the two can't disagree.
    pub fn serve_track(
        &self,
        track_id: &str,
        content_type: &ContentType,
    ) -> Result<ServedTrack, String> {
        let format = (self.stream_format)(content_type);
        Ok(ServedTrack {
            url: (self.stream_url)(track_id, format)?,
            content_type: format.content_type_str(content_type),
            cover_url: (self.cover_url)(track_id),
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
