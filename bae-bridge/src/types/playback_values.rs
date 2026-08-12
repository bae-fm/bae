use super::{BridgeImageRef, BridgeRepeatMode};

/// The target track's display metadata, carried by a loading state once core
/// has resolved it. Mirror of `bae_core::playback::LoadingTrack` across the
/// uniffi boundary.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLoadingTrackInfo {
    pub track_title: String,
    pub artist_names: String,
    pub album_id: String,
    pub album_title: String,
    /// The track's own release's cover, or `None` when it has none. Versioned,
    /// so the UI's art cache key moves when the cover bytes change.
    pub cover_image: Option<BridgeImageRef>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackState {
    Stopped,
    Loading {
        track_id: String,
        /// The target track's metadata, once resolved. `None` until core's
        /// prepare step completes.
        track: Option<BridgeLoadingTrackInfo>,
    },
    Playing {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSidePausePrompt {
    pub id: String,
    pub title_key: String,
    pub side_letter: String,
    pub message_key: String,
}

impl BridgeSidePausePrompt {
    pub(crate) fn from_core(prompt: bae_core::playback::PlaybackSidePausePrompt) -> Self {
        let bae_core::playback::PlaybackSidePausePrompt {
            id,
            title_key,
            side_letter,
            message_key,
        } = prompt;
        Self {
            id,
            title_key: title_key.to_string(),
            side_letter,
            message_key: message_key.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePlaybackPauseReason {
    Manual,
    SideEnded { prompt: BridgeSidePausePrompt },
}

impl BridgePlaybackPauseReason {
    pub(crate) fn from_core(reason: bae_core::playback::PlaybackPauseReason) -> Self {
        match reason {
            bae_core::playback::PlaybackPauseReason::Manual => Self::Manual,
            bae_core::playback::PlaybackPauseReason::SideEnded(prompt) => Self::SideEnded {
                prompt: BridgeSidePausePrompt::from_core(prompt),
            },
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePreviewState {
    Idle,
    Playing { path: String, duration_ms: u64 },
    Paused { path: String, duration_ms: u64 },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackPosition {
    pub track_id: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub progress: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePreviewValues {
    pub state: BridgePreviewState,
    pub position_ms: u64,
    pub progress: f64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackValues {
    pub state: BridgePlaybackValueState,
    pub position: Option<BridgePlaybackPosition>,
    pub seek_revision: u64,
    pub volume: f32,
    pub is_muted: bool,
    pub repeat_mode: BridgeRepeatMode,
    pub remote_device_name: Option<String>,
    pub preview: BridgePreviewValues,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackValueState {
    Stopped,
    Loading {
        track_id: String,
        track: Option<BridgeLoadingTrackInfo>,
    },
    Playing {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
        reason: BridgePlaybackPauseReason,
    },
}

impl BridgeLoadingTrackInfo {
    pub(crate) fn from_core(value: bae_core::playback::LoadingTrack) -> Self {
        Self {
            track_title: value.track_info.track_title,
            artist_names: value.track_info.artist_names,
            album_id: value.track_info.album_id,
            album_title: value.track_info.album_title,
            cover_image: value.track_info.cover_image.map(BridgeImageRef::from_core),
            duration_ms: value.duration_ms,
        }
    }
}

impl BridgePlaybackValueState {
    fn from_core(value: bae_core::playback::PlaybackState) -> Self {
        match value {
            bae_core::playback::PlaybackState::Stopped => Self::Stopped,
            bae_core::playback::PlaybackState::Loading { track_id, resolved } => Self::Loading {
                track_id,
                track: resolved.map(BridgeLoadingTrackInfo::from_core),
            },
            bae_core::playback::PlaybackState::Playing {
                track_info,
                duration_ms,
            } => Self::Playing {
                track_id: track_info.track_id,
                track_title: track_info.track_title,
                artist_names: track_info.artist_names,
                artist_id: track_info.artist_id,
                album_id: track_info.album_id,
                album_title: track_info.album_title,
                cover_image: track_info.cover_image.map(BridgeImageRef::from_core),
                duration_ms,
            },
            bae_core::playback::PlaybackState::Paused {
                track_info,
                duration_ms,
                reason,
            } => Self::Paused {
                track_id: track_info.track_id,
                track_title: track_info.track_title,
                artist_names: track_info.artist_names,
                artist_id: track_info.artist_id,
                album_id: track_info.album_id,
                album_title: track_info.album_title,
                cover_image: track_info.cover_image.map(BridgeImageRef::from_core),
                duration_ms,
                reason: BridgePlaybackPauseReason::from_core(reason),
            },
        }
    }
}

impl BridgePreviewState {
    fn from_core(value: bae_core::playback::PreviewState) -> Self {
        match value {
            bae_core::playback::PreviewState::Idle => Self::Idle,
            bae_core::playback::PreviewState::Playing { path, duration_ms } => {
                Self::Playing { path, duration_ms }
            }
            bae_core::playback::PreviewState::Paused { path, duration_ms } => {
                Self::Paused { path, duration_ms }
            }
        }
    }
}

impl BridgePlaybackValues {
    pub(crate) fn from_core(value: bae_core::playback::PlaybackValues) -> Self {
        Self {
            state: BridgePlaybackValueState::from_core(value.state),
            position: value.position.map(|position| BridgePlaybackPosition {
                track_id: position.track_id,
                position_ms: position.position_ms,
                duration_ms: position.duration_ms,
                progress: position.progress,
            }),
            seek_revision: value.seek_revision,
            volume: value.volume,
            is_muted: value.is_muted,
            repeat_mode: BridgeRepeatMode::from_core(value.repeat_mode),
            remote_device_name: value.remote_device_name,
            preview: BridgePreviewValues {
                state: BridgePreviewState::from_core(value.preview.state),
                position_ms: value.preview.position_ms,
                progress: value.preview.progress,
            },
        }
    }
}
