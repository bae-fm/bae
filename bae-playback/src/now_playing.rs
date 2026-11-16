//! What playback surfaces show of the playing track: the playback service's
//! [`PlayingTrack`], joined with the library's current [`TrackDisplay`] for it.

/// A track as the now-playing bar, the queue, and the system media controls
/// show it, read from the library's current state.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDisplay {
    pub title: String,
    pub artist_line: bae_types::credits::CreditLine,
    pub album_id: String,
    /// The release the track is on, which may be any of its album's releases.
    pub release_id: String,
    pub album_title: String,
    /// The track's own release's cover, versioned, so new bytes replace the
    /// copy a UI decoded; `None` when that release has no cover.
    pub cover_image: Option<bae_records::ImageRef>,
}

/// The playing track as the playback service knows it: which track, and the
/// duration its prepared audio plays for — `None` when its length is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayingTrack {
    pub track_id: String,
    pub duration_ms: Option<u64>,
}

/// The playing track as surfaces show it.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingTrack {
    pub track: PlayingTrack,
    pub display: TrackDisplay,
}

/// What playback surfaces show, read from the device's `playback_now` row:
/// the service's values with the playing track's current display, the
/// queue's repeat mode, and why playback last failed.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingValues {
    pub playback: crate::PlaybackValues<NowPlayingTrack>,
    pub repeat_mode: crate::RepeatMode,
    /// Why playback last failed; `None` once playback plays again or the
    /// person dismisses the failure notice.
    pub failure: Option<crate::PlaybackErrorReason>,
}
