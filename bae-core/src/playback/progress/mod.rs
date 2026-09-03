pub mod handle;
use crate::playback::service::PlaybackState;
use crate::playback::RepeatMode;
pub use handle::PlaybackProgressHandle;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;

/// Display-ready state of preview (source-window audition) playback. Carries
/// identity and duration only — position flows exclusively through
/// `PlaybackProgress::PreviewPositionUpdate`, so there is one sink for it, not
/// two.
#[derive(Debug, Clone)]
pub enum PreviewState {
    Idle,
    Playing {
        target: crate::playback::PreviewTarget,
        duration_ms: u64,
    },
    Paused {
        target: crate::playback::PreviewTarget,
        duration_ms: u64,
    },
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

#[derive(Debug, Clone)]
pub struct PlaybackPosition {
    pub track_id: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub progress: f64,
}

#[derive(Debug, Clone)]
pub struct PreviewValues {
    pub state: PreviewState,
    pub position_ms: u64,
    pub progress: f64,
}

/// The one playback presentation an operating-system media surface should
/// show. Core resolves library versus preview ownership so platform adapters
/// never race two independent players onto one system slot.
#[derive(Debug, Clone)]
pub struct MediaControlValues {
    pub playback: MediaControlPlayback,
    pub volume: f32,
    pub is_muted: bool,
}

#[derive(Debug, Clone)]
pub enum MediaControlPlayback {
    Library {
        state: PlaybackState,
        position: Option<PlaybackPosition>,
        seek_revision: u64,
    },
    Preview {
        target: crate::playback::PreviewTarget,
        duration_ms: u64,
        position_ms: u64,
        is_playing: bool,
    },
}

#[derive(Debug, Clone)]
pub struct PlaybackValues {
    pub state: PlaybackState,
    pub position: Option<PlaybackPosition>,
    /// Monotonic acknowledgement of an applied seek. It remains at the latest
    /// value across ordinary position ticks so consumers cannot miss the seek
    /// acknowledgement when watch delivery coalesces updates.
    pub seek_revision: u64,
    pub volume: f32,
    pub is_muted: bool,
    pub repeat_mode: RepeatMode,
    pub remote_device_name: Option<String>,
    pub preview: PreviewValues,
}

impl PlaybackValues {
    pub(super) fn initial() -> Self {
        Self {
            state: PlaybackState::Stopped,
            position: None,
            seek_revision: 0,
            volume: 1.0,
            is_muted: false,
            repeat_mode: RepeatMode::Off,
            remote_device_name: None,
            preview: PreviewValues {
                state: PreviewState::Idle,
                position_ms: 0,
                progress: 0.0,
            },
        }
    }

    pub(super) fn applying(&self, event: &PlaybackProgress) -> Option<Self> {
        let mut next = self.clone();
        match event {
            PlaybackProgress::StateChanged { state } => {
                if playback_track_id(&next.state) != playback_track_id(state) {
                    next.position = None;
                }
                next.state = state.clone();
            }
            PlaybackProgress::PositionUpdate {
                track_id,
                position_ms,
                duration_ms,
                progress,
            } => {
                next.position = Some(PlaybackPosition {
                    track_id: track_id.clone(),
                    position_ms: *position_ms,
                    duration_ms: *duration_ms,
                    progress: *progress,
                });
            }
            PlaybackProgress::Seeked {
                track_id,
                position_ms,
                duration_ms,
                progress,
            } => {
                next.position = Some(PlaybackPosition {
                    track_id: track_id.clone(),
                    position_ms: *position_ms,
                    duration_ms: *duration_ms,
                    progress: *progress,
                });
                next.seek_revision = next
                    .seek_revision
                    .checked_add(1)
                    .expect("playback seek revision overflow");
            }
            PlaybackProgress::RepeatModeChanged { mode } => next.repeat_mode = *mode,
            PlaybackProgress::VolumeChanged { volume } => next.volume = *volume,
            PlaybackProgress::MuteChanged { is_muted } => next.is_muted = *is_muted,
            PlaybackProgress::RemoteStatusChanged { device_name } => {
                next.remote_device_name.clone_from(device_name);
            }
            PlaybackProgress::PreviewStateChanged(state) => {
                next.preview.state = state.clone();
                if matches!(state, PreviewState::Idle) {
                    next.preview.position_ms = 0;
                    next.preview.progress = 0.0;
                }
            }
            PlaybackProgress::PreviewPositionUpdate {
                position_ms,
                progress,
            } => {
                next.preview.position_ms = *position_ms;
                next.preview.progress = *progress;
            }
            PlaybackProgress::QueueItemsAdded { .. }
            | PlaybackProgress::PlaybackError { .. }
            | PlaybackProgress::TrackCompleted { .. }
            | PlaybackProgress::DecodeStats { .. } => return None,
        }
        Some(next)
    }

    pub fn media_control_values(&self) -> MediaControlValues {
        let playback = match &self.preview.state {
            PreviewState::Playing {
                target,
                duration_ms,
            } => MediaControlPlayback::Preview {
                target: target.clone(),
                duration_ms: *duration_ms,
                position_ms: self.preview.position_ms,
                is_playing: true,
            },
            PreviewState::Paused {
                target,
                duration_ms,
            } => MediaControlPlayback::Preview {
                target: target.clone(),
                duration_ms: *duration_ms,
                position_ms: self.preview.position_ms,
                is_playing: false,
            },
            PreviewState::Idle => MediaControlPlayback::Library {
                state: self.state.clone(),
                position: self.position.clone(),
                seek_revision: self.seek_revision,
            },
        };
        MediaControlValues {
            playback,
            volume: self.volume,
            is_muted: self.is_muted,
        }
    }
}

fn playback_track_id(state: &PlaybackState) -> Option<&str> {
    match state {
        PlaybackState::Stopped => None,
        PlaybackState::Loading { track_id, .. } => Some(track_id),
        PlaybackState::Playing { track_info, .. } | PlaybackState::Paused { track_info, .. } => {
            Some(&track_info.track_id)
        }
    }
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
    /// The active renderer changed: `Some(name)` when playback moved to a remote
    /// renderer (Cast or DLNA), `None` when it returned to local output (a user
    /// stop or a device-side end). The UI reflects the speaker button's active
    /// state and the "Playing on `<name>`" row from this.
    RemoteStatusChanged {
        device_name: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_control_uses_preview_only_while_preview_is_active() {
        let library = PlaybackValues::initial();
        assert!(matches!(
            library.media_control_values().playback,
            MediaControlPlayback::Library {
                state: PlaybackState::Stopped,
                ..
            }
        ));

        let preview = library
            .applying(&PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing {
                    target: crate::playback::PreviewTarget::sample_range(
                        "/tmp/preview.flac".into(),
                        0,
                        None,
                    ),
                    duration_ms: 120_000,
                },
            ))
            .expect("preview state is retained");
        assert!(matches!(
            preview.media_control_values().playback,
            MediaControlPlayback::Preview {
                duration_ms: 120_000,
                is_playing: true,
                ..
            }
        ));

        let idle = preview
            .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Idle))
            .expect("idle state is retained");
        assert!(matches!(
            idle.media_control_values().playback,
            MediaControlPlayback::Library {
                state: PlaybackState::Stopped,
                ..
            }
        ));
    }
}
