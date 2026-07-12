pub mod handle;
use crate::playback::service::PlaybackState;
use crate::playback::RepeatMode;
pub use handle::PlaybackProgressHandle;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;

/// Display-ready state of preview (file audition) playback. Carries identity
/// (path) and duration only — position flows exclusively through
/// `PlaybackProgress::PreviewPositionUpdate`, so there is one sink for it, not
/// two.
#[derive(Debug, Clone)]
pub enum PreviewState {
    Idle,
    Playing { path: String, duration_ms: u64 },
    Paused { path: String, duration_ms: u64 },
}

/// The queue shape owned by the playback loop: per-instance queue entries,
/// context metadata, and navigation affordances. The library layer resolves the
/// entries to display rows because it owns track metadata.
#[derive(Debug, Clone)]
pub struct PlaybackQueueProjection {
    pub manual: Vec<crate::playback::QueueEntry>,
    pub context: Option<crate::playback::ContextProjection>,
    pub has_next: bool,
    pub has_previous: bool,
    /// The `PlaybackQueue` revision this projection was read at. Stamped onto
    /// the resolved snapshot and onto every upcoming-page fetch so a UI can
    /// tell whether a page still corresponds to the queue it is rendering.
    pub revision: u64,
}

/// Progress updates during playback. The variants below the "Internal events"
/// divider are consumed by `PlaybackService` itself (`TrackCompleted` drives
/// auto-advance); external subscribers can ignore them, since the transitions
/// they cause emit their own `StateChanged`.
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
    },
    /// The queue changed. The two lanes stay separate: the manual lane ("Up
    /// Next") in order, and the context (the release being played from) as its
    /// not-yet-played tail plus its shuffled flag, or `None` when nothing plays
    /// from a release. Each entry is a per-instance id + track id; the event bus
    /// resolves them to display items.
    QueueUpdated(PlaybackQueueProjection),
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
    /// Playback error (e.g. storage offline) — a typed reason the UI renders
    /// for its locale.
    PlaybackError {
        reason: crate::ui::PlaybackErrorReason,
    },

    // -- Internal events --
    /// The track drained. Drives auto-advance.
    TrackCompleted {
        track_id: String,
    },
    /// Position moved within the same track. Emitted by a seek, and by `restore`
    /// and the pause/resume refresh, which need the display repositioned without
    /// a state change.
    Seeked {
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), paired with
        /// `position_ms` so consumers don't re-derive it from track state.
        duration_ms: u64,
        track_id: String,
        progress: f64,
    },
    /// Decode stats for a track that finished or was stopped.
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
