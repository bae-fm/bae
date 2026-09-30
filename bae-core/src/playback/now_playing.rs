//! What playback surfaces show of the playing track: the playback service's
//! [`PlayingTrack`], joined with the library's current [`TrackDisplay`] for it.

/// A track as the now-playing bar, the queue, and the system media controls
/// show it, read from the library's current state.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDisplay {
    pub title: String,
    pub artist_names: String,
    pub album_id: String,
    /// The release the track is on, which may be any of its album's releases.
    pub release_id: String,
    pub album_title: String,
    /// The track's own release's cover, versioned, so new bytes replace the
    /// copy a UI decoded; `None` when that release has no cover.
    pub cover_image: Option<crate::album_detail::ImageRef>,
}

/// The playing track as the playback service knows it: which track, and the
/// duration its prepared audio plays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayingTrack {
    pub track_id: String,
    pub duration_ms: u64,
}

/// The playing track as surfaces show it.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingTrack {
    pub track: PlayingTrack,
    pub display: TrackDisplay,
}

/// The playback values surfaces show: the service's values with the playing
/// track's current display.
pub type NowPlayingValues = crate::playback::PlaybackValues<NowPlayingTrack>;
