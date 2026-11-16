//! What the apps' playback screens show of the track the playback row names:
//! the playback service's [`PlayingTrack`] or the id of the track it loads,
//! with the library's current [`TrackDisplay`] for it.
//! [`NowPlayingDisplay::of`] marks the track removed when the library no
//! longer holds it.

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

/// What surfaces show of the track the playback row names: the library's
/// current display of it, or that the library no longer holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum NowPlayingDisplay {
    Library(TrackDisplay),
    /// The library no longer holds the track: the person deleted it from the
    /// library, and playback has not yet moved to another track.
    Removed,
}

impl NowPlayingDisplay {
    /// `display` when the library holds the track, and `Removed` when it
    /// read none.
    pub fn of(display: Option<TrackDisplay>) -> Self {
        display.map_or(Self::Removed, Self::Library)
    }
}

/// A track the playback row names, as surfaces show it. `Track` is what
/// playback knows of it: the prepared [`PlayingTrack`], or the track's id
/// alone while a load has yet to prepare it.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingTrack<Track = PlayingTrack> {
    pub track: Track,
    pub display: NowPlayingDisplay,
}

/// What playback surfaces show, read from the device's `playback_now` row:
/// the service's values with the current display of the track the row
/// names, the queue's repeat mode, and why playback last failed.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingValues {
    pub playback: crate::PlaybackValues<NowPlayingTrack, NowPlayingTrack<String>>,
    pub repeat_mode: crate::RepeatMode,
    /// Why playback last failed; `None` once playback plays again or the
    /// person dismisses the failure notice.
    pub failure: Option<crate::PlaybackErrorReason>,
}
