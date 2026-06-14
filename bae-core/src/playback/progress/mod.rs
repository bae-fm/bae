pub mod handle;
use crate::playback::service::PlaybackState;
use crate::playback::RepeatMode;
pub use handle::PlaybackProgressHandle;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;

/// State of preview (file audition) playback — display-ready.
///
/// Carries **identity** (path) and **metadata** (duration) only. Position
/// data flows exclusively through `PlaybackProgress::PreviewPositionUpdate`
/// events so there is one sink for position, not two. Unlike main playback,
/// preview has no restore-on-launch case, so no late-mount cache is needed.
#[derive(Debug, Clone)]
pub enum PreviewState {
    Idle,
    Playing {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
    Paused {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
}

/// Progress updates during playback.
///
/// Some variants are **internal** — consumed by PlaybackService itself
/// (e.g. TrackCompleted drives auto-advance) and don't need external handling
/// because the resulting state transitions produce StateChanged events.
#[derive(Debug, Clone)]
pub enum PlaybackProgress {
    StateChanged {
        state: PlaybackState,
    },
    PositionUpdate {
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), paired with
        /// `position_ms` so consumers don't re-derive it from track state.
        duration_ms: u64,
        track_id: String,
        progress: f64,
        elapsed_label: String,
        remaining_label: String,
    },
    /// Queue was updated — contains current queue state
    QueueUpdated {
        tracks: Vec<String>,
        has_next: bool,
        has_previous: bool,
    },
    /// Tracks were appended/inserted into the queue. Fired only by add
    /// operations; never by remove/reorder/clear. The UI surfaces this as
    /// a transient "+N" badge.
    QueueItemsAdded {
        count: u32,
    },
    RepeatModeChanged {
        mode: RepeatMode,
    },
    VolumeChanged {
        volume: f32,
    },
    MuteChanged {
        is_muted: bool,
    },
    /// Playback error (e.g. storage offline)
    PlaybackError {
        message: String,
    },

    // -- Internal events --
    // Consumed by PlaybackService for control flow (auto-advance, etc.).
    // External subscribers can ignore these; StateChanged covers the
    // resulting UI-visible transitions.
    /// Internal: track finished decoding. Triggers auto-advance.
    TrackCompleted {
        track_id: String,
    },
    /// Seek completed, position changed within the same track.
    /// Also emitted by restore() and helper paths that need to refresh
    /// position display without a full state change.
    Seeked {
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), paired with
        /// `position_ms` so consumers don't re-derive it from track state.
        duration_ms: u64,
        track_id: String,
        progress: f64,
        elapsed_label: String,
        remaining_label: String,
    },
    /// Internal: seek skipped because position difference was < 100ms.
    SeekSkipped {
        requested_position: Duration,
        current_position: Duration,
    },
    /// Internal: decode stats emitted when a track finishes or is stopped.
    DecodeStats {
        track_id: String,
        error_count: u32,
        samples_decoded: u64,
    },
    /// Preview state changed (playing, paused, finished, idle).
    PreviewStateChanged(PreviewState),
    /// Preview position tick (decoupled from state so ticks don't imply playing).
    PreviewPositionUpdate {
        position_ms: u64,
        progress: f64,
        elapsed_label: String,
    },
}

/// Emit a progress event to subscribers. Logs at warn-level if no subscribers
/// remain (the progress fan-out task has shut down).
pub(crate) fn emit_progress(
    tx: &tokio_mpsc::UnboundedSender<PlaybackProgress>,
    event: PlaybackProgress,
) {
    if let Err(err) = tx.send(event) {
        warn!("playback progress channel closed; dropped {:?}", err.0);
    }
}
