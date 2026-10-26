use super::{BridgeImageRef, BridgeRepeatMode};

/// One local file range to audition, by sample bounds only: an import
/// candidate has no byte seek positions yet.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePreviewTarget {
    pub path: String,
    pub start_sample: u64,
    pub end_sample: Option<u64>,
}

impl BridgePreviewTarget {
    pub(crate) fn from_core(target: bae_core::playback::PreviewTarget) -> Self {
        let (path, start_sample, end_sample) = target.into_sample_range();
        Self {
            path,
            start_sample,
            end_sample,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::playback::PreviewTarget {
        bae_core::playback::PreviewTarget::sample_range(
            self.path,
            self.start_sample,
            self.end_sample,
        )
    }
}

/// The playing track as surfaces show it: the playback service's track joined
/// with the library's current display for it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeNowPlayingTrack {
    pub track: BridgePlayingTrack,
    pub display: BridgeTrackDisplay,
}

/// The playing track as the playback service knows it: which track, and the
/// duration its prepared audio plays for — `None` when its length is unknown.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlayingTrack {
    pub track_id: String,
    pub duration_ms: Option<u64>,
}

/// A track as the now-playing bar, the queue, and the system media controls
/// show it, read from the library's current state.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackDisplay {
    pub title: String,
    pub artist_names: String,
    pub album_id: String,
    /// The release the track is on, which may be any of its album's releases.
    pub release_id: String,
    pub album_title: String,
    /// The track's own release's cover, versioned so the UI's art cache key
    /// changes with the cover bytes; `None` when that release has no cover.
    pub cover_image: Option<BridgeImageRef>,
}

mirror_struct! {
    BridgeNowPlayingTrack = bae_core::playback::NowPlayingTrack,
    from_core: fn,
    fields: { track: (BridgePlayingTrack), display: (BridgeTrackDisplay) },
}

mirror_struct! {
    BridgePlayingTrack = bae_core::playback::PlayingTrack,
    from_core: fn,
    fields: { track_id, duration_ms },
}

mirror_struct! {
    BridgeTrackDisplay = bae_core::playback::TrackDisplay,
    from_core: pub(crate) fn,
    fields: {
        title,
        artist_names,
        album_id,
        release_id,
        album_title,
        cover_image: (opt BridgeImageRef),
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSidePausePrompt {
    pub id: String,
    /// Whether a side or a disc ended, which every line of the prompt names.
    pub boundary: BridgePauseBoundary,
    pub side_label: String,
    /// The countdown to the next side starting on its own, or `None` when the
    /// pause waits for Play.
    pub countdown: Option<BridgeSideCountdown>,
}

impl BridgeSidePausePrompt {
    pub(crate) fn from_core(prompt: bae_core::playback::PlaybackSidePausePrompt) -> Self {
        let bae_core::playback::PlaybackSidePausePrompt {
            id,
            boundary,
            side_label,
            countdown,
        } = prompt;
        Self {
            id,
            boundary: BridgePauseBoundary::from_core(boundary),
            side_label,
            countdown: countdown.map(BridgeSideCountdown::from_core),
        }
    }
}

/// Whether a pause between sides stopped after a side or a disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePauseBoundary {
    Side,
    Disc,
}

mirror_enum! {
    BridgePauseBoundary = bae_core::playback::PlaybackPauseBoundary,
    from_core: fn,
    variants: {
        Side,
        Disc,
    },
}

/// The prompt's title, which takes the side or disc that ended as `label`.
#[uniffi::export]
pub fn bridge_pause_boundary_title_key(boundary: BridgePauseBoundary) -> String {
    match boundary {
        BridgePauseBoundary::Side => "core.playback.pause.side_ended.title",
        BridgePauseBoundary::Disc => "core.playback.pause.disc_ended.title",
    }
    .to_string()
}

/// The line counting down to the next side, which takes the whole seconds
/// left, rounded up, as `seconds`.
#[uniffi::export]
pub fn bridge_pause_boundary_countdown_key(boundary: BridgePauseBoundary) -> String {
    match boundary {
        BridgePauseBoundary::Side => "core.playback.pause.side_ended.countdown",
        BridgePauseBoundary::Disc => "core.playback.pause.disc_ended.countdown",
    }
    .to_string()
}

/// The prompt's checkbox that keeps pausing at boundaries of this kind.
#[uniffi::export]
pub fn bridge_pause_boundary_keep_pausing_key(boundary: BridgePauseBoundary) -> String {
    match boundary {
        BridgePauseBoundary::Side => "core.playback.pause.side_ended.keep_pausing",
        BridgePauseBoundary::Disc => "core.playback.pause.disc_ended.keep_pausing",
    }
    .to_string()
}

/// A running side-pause countdown. Core starts the next side; UIs only count
/// down to `resumes_at_ms`, so they all show the same number.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSideCountdown {
    /// When the next side starts, as Unix epoch milliseconds.
    pub resumes_at_ms: i64,
}

impl BridgeSideCountdown {
    fn from_core(countdown: bae_core::playback::PlaybackSideCountdown) -> Self {
        let bae_core::playback::PlaybackSideCountdown { resumes_at } = countdown;
        Self {
            resumes_at_ms: resumes_at.timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePlaybackPauseReason {
    Manual,
    SideEnded { prompt: BridgeSidePausePrompt },
}

mirror_enum! {
    BridgePlaybackPauseReason = bae_core::playback::PlaybackPauseReason,
    from_core: pub(crate) fn,
    variants: {
        Manual,
        SideEnded(prompt: (BridgeSidePausePrompt)),
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePreviewState {
    Idle,
    Playing {
        target: BridgePreviewTarget,
        duration_ms: u64,
    },
    Paused {
        target: BridgePreviewTarget,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackPosition {
    pub track_id: String,
    pub position_ms: i64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    /// The seek bar's fill; zero throughout when the length is unknown.
    pub progress: f64,
}

/// The library timeline exposed to an operating-system media surface. Unlike
/// the in-app position, it cannot represent the pregap countdown because those
/// APIs accept only positions at or after track start.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMediaControlPosition {
    pub track_id: String,
    pub position_ms: u64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    pub progress: f64,
}

/// Where the preview is in its clip, and which clip that is.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePreviewPosition {
    pub target: BridgePreviewTarget,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub progress: f64,
}

/// What the operating-system media surface shows. Mirrors
/// `bae_core::playback::MediaControlPlayback`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMediaControlPlayback {
    /// The library playback, whose state is [`BridgePlaybackValues::state`].
    Library,
    Preview {
        target: BridgePreviewTarget,
        duration_ms: u64,
        is_playing: bool,
    },
}

/// The remote renderer playback is on: its stable id, which the UI matches
/// against the device list, and its name, which it shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRemoteDevice {
    pub id: String,
    pub name: String,
}

mirror_struct! {
    BridgeRemoteDevice = bae_core::playback::RemoteDevice,
    from_core: pub(crate) fn,
    fields: { id, name },
}

/// Where playback is going, for the cast surfaces: this device's own output,
/// a remote renderer, or back here after the link to a renderer was lost,
/// with why (untranslated, for a details disclosure). The UI words the ended
/// line itself (`core.cast.ended`, with the device's name). Mirrors
/// `bae_core::playback::RemotePlayback`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeRemotePlayback {
    Local,
    On {
        device: BridgeRemoteDevice,
    },
    Ended {
        device: BridgeRemoteDevice,
        detail: String,
    },
}

mirror_enum! {
    BridgeRemotePlayback = bae_core::playback::RemotePlayback,
    from_core: pub(crate) fn,
    variants: {
        Local,
        On(device: (BridgeRemoteDevice)),
        Ended { device: (BridgeRemoteDevice), detail },
    },
}

/// What playback is doing, apart from where it is in what it plays; sent
/// only when it changes. Mirrors `bae_core::playback::NowPlayingValues`, with
/// the media controls' presentation resolved from it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackValues {
    pub state: BridgePlaybackValueState,
    pub volume: f32,
    pub is_muted: bool,
    pub repeat_mode: BridgeRepeatMode,
    pub remote: BridgeRemotePlayback,
    pub preview: BridgePreviewState,
    pub media_control: BridgeMediaControlPlayback,
}

/// Where playback is in what it plays; sent as it moves. Each position names
/// its track or clip. Mirrors `bae_core::playback::PlaybackPositions`, with
/// the library position on the media controls' timeline.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackPositions {
    pub library: Option<BridgePlaybackPosition>,
    pub seek_revision: u64,
    pub preview: Option<BridgePreviewPosition>,
    pub media_control: Option<BridgeMediaControlPosition>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackValueState {
    Stopped,
    Loading {
        track: BridgeLoadingTrack,
    },
    Playing {
        track: BridgeNowPlayingTrack,
    },
    Paused {
        track: BridgeNowPlayingTrack,
        reason: BridgePlaybackPauseReason,
    },
}

mirror_enum! {
    BridgePlaybackValueState = bae_core::playback::PlaybackState<bae_core::playback::NowPlayingTrack>,
    from_core: fn,
    variants: {
        Stopped,
        Loading { track: (BridgeLoadingTrack) },
        Playing { track: (BridgeNowPlayingTrack) },
        Paused { track: (BridgeNowPlayingTrack), reason: (BridgePlaybackPauseReason) },
    },
}

/// The track a load is for: its id alone until core has prepared it, then the
/// prepared track, which carries the id.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLoadingTrack {
    Unprepared { track_id: String },
    Prepared { track: BridgeNowPlayingTrack },
}

mirror_enum! {
    BridgeLoadingTrack = bae_core::playback::LoadingTrack<bae_core::playback::NowPlayingTrack>,
    from_core: fn,
    variants: {
        Unprepared { track_id },
        Prepared(track: (BridgeNowPlayingTrack)),
    },
}

mirror_struct! {
    BridgePlaybackPosition = bae_core::playback::PlaybackPosition,
    from_core: fn,
    fields: { track_id, position_ms, duration_ms, progress },
}

mirror_struct! {
    BridgeMediaControlPosition = bae_core::playback::MediaControlPosition,
    from_core: fn,
    fields: { track_id, position_ms, duration_ms, progress },
}

mirror_struct! {
    BridgePreviewPosition = bae_core::playback::PreviewPosition,
    from_core: fn,
    fields: { target: (BridgePreviewTarget), duration_ms, position_ms, progress },
}

mirror_enum! {
    BridgePreviewState = bae_core::playback::PreviewState,
    from_core: fn,
    variants: {
        Idle,
        Playing { target: (BridgePreviewTarget), duration_ms },
        Paused { target: (BridgePreviewTarget), duration_ms },
    },
}

impl BridgePlaybackValues {
    pub(crate) fn from_core(value: bae_core::playback::NowPlayingValues) -> Self {
        let media_control = BridgeMediaControlPlayback::from_core(value.media_control());
        Self {
            state: BridgePlaybackValueState::from_core(value.state),
            volume: value.volume,
            is_muted: value.is_muted,
            repeat_mode: BridgeRepeatMode::from_core(value.repeat_mode),
            remote: BridgeRemotePlayback::from_core(value.remote),
            preview: BridgePreviewState::from_core(value.preview),
            media_control,
        }
    }
}

impl BridgePlaybackPositions {
    pub(crate) fn from_core(positions: bae_core::playback::PlaybackPositions) -> Self {
        let media_control = positions
            .media_control()
            .map(BridgeMediaControlPosition::from_core);
        Self {
            library: positions.library.map(BridgePlaybackPosition::from_core),
            seek_revision: positions.seek_revision,
            preview: positions.preview.map(BridgePreviewPosition::from_core),
            media_control,
        }
    }
}

mirror_enum! {
    BridgeMediaControlPlayback = bae_core::playback::MediaControlPlayback,
    from_core: fn,
    variants: {
        Library,
        Preview {
            target: (BridgePreviewTarget),
            duration_ms,
            is_playing,
        },
    },
}
