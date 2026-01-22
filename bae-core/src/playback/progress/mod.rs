pub mod handle;
use crate::playback::service::{PlaybackState, RepeatMode};
pub use handle::PlaybackProgressHandle;
use std::time::Duration;
/// Progress updates during playback
#[derive(Debug, Clone)]
pub enum PlaybackProgress {
    StateChanged {
        state: PlaybackState,
    },
    PositionUpdate {
        position: Duration,
        track_id: String,
    },
    TrackCompleted {
        track_id: String,
    },
    /// Seek completed successfully - position changed within the same track
    /// UI should update position and clear is_seeking flag
    Seeked {
        position: Duration,
        track_id: String,
        was_paused: bool,
    },
    SeekError {
        requested_position: Duration,
        track_duration: Duration,
    },
    /// Seek was skipped because position difference was too small (< 100ms)
    /// UI should clear is_seeking flag
    SeekSkipped {
        requested_position: Duration,
        current_position: Duration,
    },
    /// Queue was updated - contains current queue state
    QueueUpdated {
        tracks: Vec<String>,
    },
    /// Repeat mode changed
    RepeatModeChanged {
        mode: RepeatMode,
    },
    /// Playback error occurred (e.g. storage offline)
    PlaybackError {
        message: String,
    },
    /// Decode statistics for completed/stopped track
    /// Sent when track finishes or is stopped, includes FFmpeg error count
    DecodeStats {
        track_id: String,
        /// Number of fatal FFmpeg decode errors
        error_count: u32,
        /// Total samples decoded (to verify audio was actually produced)
        samples_decoded: u64,
    },
}
