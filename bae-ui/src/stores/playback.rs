//! Playback UI state store

use crate::display_types::QueueItem;
use dioxus::prelude::*;

/// Playback state enum matching bae-core's PlaybackState
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
}

/// Repeat mode for playback
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RepeatMode {
    #[default]
    None,
    Track,
    Album,
}

/// UI state for playback
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct PlaybackUiState {
    /// Current playback state
    pub status: PlaybackStatus,
    /// Queue of track IDs (raw IDs from playback service)
    pub queue: Vec<String>,
    /// Currently playing track ID
    pub current_track_id: Option<String>,
    /// Release ID for navigation (needed to navigate to album page)
    pub current_release_id: Option<String>,
    /// Current track display info (track + album title + cover)
    pub current_track: Option<QueueItem>,
    /// Queue items with full display info (track + album title + cover)
    pub queue_items: Vec<QueueItem>,
    /// Current playback position in milliseconds
    pub position_ms: u64,
    /// Track duration in milliseconds (0 if unknown)
    pub duration_ms: u64,
    /// Track pregap in milliseconds (for CUE tracks)
    pub pregap_ms: Option<i64>,
    /// Artist name for current track
    pub artist_name: String,
    /// Cover art URL for current track
    pub cover_url: Option<String>,
    /// Transient playback error message
    pub playback_error: Option<String>,
    /// Repeat mode
    pub repeat_mode: RepeatMode,
}
