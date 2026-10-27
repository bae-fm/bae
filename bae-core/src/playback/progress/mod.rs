pub mod handle;
use crate::playback::service::PlaybackState;
use crate::playback::RepeatMode;
pub use handle::PlaybackProgressHandle;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;

/// Display-ready state of preview (source-window audition) playback. Carries
/// identity and duration only — its position is [`PreviewPosition`], which
/// moves while the preview plays.
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackPosition {
    pub track_id: String,
    /// Track-relative position: negative while pregap audio plays, zero at
    /// INDEX 01, positive afterward.
    pub position_ms: i64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    /// The seek bar's fill; zero throughout when the length is unknown.
    pub progress: f64,
}

/// The non-negative timeline accepted by operating-system media controls.
#[derive(Debug, Clone)]
pub struct MediaControlPosition {
    pub track_id: String,
    pub position_ms: u64,
    /// `None` when the track's length is unknown.
    pub duration_ms: Option<u64>,
    pub progress: f64,
}

impl MediaControlPosition {
    fn from_playback(position: &PlaybackPosition) -> Self {
        Self {
            track_id: position.track_id.clone(),
            position_ms: position.position_ms.max(0) as u64,
            duration_ms: position.duration_ms,
            progress: position.progress,
        }
    }
}

/// Where the preview is in its clip, and which clip that is, so a surface
/// never shows one preview's position against another preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewPosition {
    pub target: crate::playback::PreviewTarget,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub progress: f64,
}

/// The one playback presentation an operating-system media surface should
/// show. Core resolves library versus preview ownership so platform adapters
/// never race two independent players onto one system slot. Where in the
/// track it is comes from [`PlaybackPositions`]: a surface shows the
/// position whose track or clip it presents.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaControlPlayback {
    /// The library playback, whose state is [`PlaybackValues::state`].
    Library,
    Preview {
        target: crate::playback::PreviewTarget,
        duration_ms: u64,
        is_playing: bool,
    },
}

/// What playback is doing, apart from where it is in what it plays: these
/// change when playback does something, while [`PlaybackPositions`] move
/// as audio plays. `Track` is what the state holds of the playing track; see
/// [`PlaybackState`].
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackValues<Track = crate::playback::PlayingTrack> {
    pub state: PlaybackState<Track>,
    pub volume: f32,
    pub is_muted: bool,
    pub repeat_mode: RepeatMode,
    /// Where playback is going: this device's own output, a remote renderer,
    /// or back here after the link to a renderer was lost.
    pub remote: RemotePlayback,
    pub preview: PreviewState,
}

/// Where playback is in what it plays: the library track's position and the
/// preview's. Each names the track or clip it is a position of.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackPositions {
    /// `None` until the playing track reports a position, and again once
    /// playback moves to another track or stops.
    pub library: Option<PlaybackPosition>,
    /// Monotonic acknowledgement of an applied seek. It remains at the latest
    /// value across ordinary position ticks so consumers cannot miss the seek
    /// acknowledgement when watch delivery coalesces updates.
    pub seek_revision: u64,
    /// `None` while no preview is loaded.
    pub preview: Option<PreviewPosition>,
}

/// A remote renderer playback moved to: its stable id, which the UI matches
/// against the device list, and its name, which it shows. Names need not be
/// unique across devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
}

/// Where playback is going, for the cast surfaces: the speaker button's state,
/// the picker's active device, and why the last device stopped playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemotePlayback {
    /// This device's own output.
    Local,
    /// A remote renderer (Cast, DLNA or AirPlay).
    On(RemoteDevice),
    /// Back on this device's own output because the link to `device` was lost:
    /// the receiver stopped its app, went unreachable, or the network dropped.
    /// `detail` is why, untranslated. Held until playback moves to a device
    /// again, so the picker can say what happened to the last one.
    Ended {
        device: RemoteDevice,
        detail: String,
    },
}

impl PlaybackValues {
    pub(super) fn initial() -> Self {
        Self {
            state: PlaybackState::Stopped,
            volume: 1.0,
            is_muted: false,
            repeat_mode: RepeatMode::Off,
            remote: RemotePlayback::Local,
            preview: PreviewState::Idle,
        }
    }

    /// These values after `event`, or `None` when `event` is not about
    /// them.
    pub(super) fn applying(&self, event: &PlaybackProgress) -> Option<Self> {
        let mut next = self.clone();
        match event {
            PlaybackProgress::StateChanged { state } => next.state = state.clone(),
            PlaybackProgress::RepeatModeChanged { mode } => next.repeat_mode = *mode,
            PlaybackProgress::VolumeChanged { volume } => next.volume = *volume,
            PlaybackProgress::MuteChanged { is_muted } => next.is_muted = *is_muted,
            PlaybackProgress::RemoteStatusChanged { remote } => {
                next.remote.clone_from(remote);
            }
            PlaybackProgress::PreviewStateChanged(state) => next.preview = state.clone(),
            PlaybackProgress::PositionUpdate { .. }
            | PlaybackProgress::Seeked { .. }
            | PlaybackProgress::PreviewPositionUpdate { .. }
            | PlaybackProgress::QueueItemsAdded { .. }
            | PlaybackProgress::PlaybackError { .. }
            | PlaybackProgress::TrackCompleted { .. }
            | PlaybackProgress::DecodeStats { .. } => return None,
        }
        Some(next)
    }
}

impl PlaybackPositions {
    pub(super) fn initial() -> Self {
        Self {
            library: None,
            seek_revision: 0,
            preview: None,
        }
    }

    /// These positions after `event`, or `None` when `event` is not about
    /// them.
    pub(super) fn applying(&self, event: &PlaybackProgress) -> Option<Self> {
        let mut next = self.clone();
        match event {
            PlaybackProgress::StateChanged { state } => {
                // A position is of its track: a state on another track, or
                // none, ends it.
                if next
                    .library
                    .as_ref()
                    .is_some_and(|position| state.track_id() != Some(position.track_id.as_str()))
                {
                    next.library = None;
                }
            }
            PlaybackProgress::PositionUpdate {
                track_id,
                position_ms,
                duration_ms,
                progress,
            } => {
                next.library = Some(PlaybackPosition {
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
                next.library = Some(PlaybackPosition {
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
            PlaybackProgress::PreviewStateChanged(PreviewState::Idle) => next.preview = None,
            PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing {
                    target,
                    duration_ms,
                }
                | PreviewState::Paused {
                    target,
                    duration_ms,
                },
            ) => {
                // The same clip pausing, resuming or reloading for a seek
                // keeps its place; another clip starts from its beginning.
                let (position_ms, progress) = next
                    .preview
                    .as_ref()
                    .filter(|preview| preview.target == *target)
                    .map_or((0, 0.0), |preview| (preview.position_ms, preview.progress));
                next.preview = Some(PreviewPosition {
                    target: target.clone(),
                    duration_ms: *duration_ms,
                    position_ms,
                    progress,
                });
            }
            PlaybackProgress::PreviewPositionUpdate {
                position_ms,
                progress,
            } => {
                let Some(preview) = next.preview.as_mut() else {
                    tracing::debug!(
                        "preview position {position_ms} ms arrived with no preview loaded; dropped"
                    );
                    return None;
                };
                preview.position_ms = *position_ms;
                preview.progress = *progress;
            }
            PlaybackProgress::RepeatModeChanged { .. }
            | PlaybackProgress::VolumeChanged { .. }
            | PlaybackProgress::MuteChanged { .. }
            | PlaybackProgress::RemoteStatusChanged { .. }
            | PlaybackProgress::QueueItemsAdded { .. }
            | PlaybackProgress::PlaybackError { .. }
            | PlaybackProgress::TrackCompleted { .. }
            | PlaybackProgress::DecodeStats { .. } => return None,
        }
        Some(next)
    }

    /// The library position on the timeline operating-system media controls
    /// accept.
    pub fn media_control(&self) -> Option<MediaControlPosition> {
        self.library
            .as_ref()
            .map(MediaControlPosition::from_playback)
    }
}

impl<Track> PlaybackValues<Track> {
    /// The same values holding `f`'s result for the state's track, or `f`'s
    /// error.
    pub fn try_map_track<Mapped, Error>(
        self,
        f: impl FnMut(Track) -> Result<Mapped, Error>,
    ) -> Result<PlaybackValues<Mapped>, Error> {
        let Self {
            state,
            volume,
            is_muted,
            repeat_mode,
            remote,
            preview,
        } = self;
        Ok(PlaybackValues {
            state: state.try_map_track(f)?,
            volume,
            is_muted,
            repeat_mode,
            remote,
            preview,
        })
    }

    /// What the operating-system media surface shows of these values.
    pub fn media_control(&self) -> MediaControlPlayback {
        match &self.preview {
            PreviewState::Playing {
                target,
                duration_ms,
            } => MediaControlPlayback::Preview {
                target: target.clone(),
                duration_ms: *duration_ms,
                is_playing: true,
            },
            PreviewState::Paused {
                target,
                duration_ms,
            } => MediaControlPlayback::Preview {
                target: target.clone(),
                duration_ms: *duration_ms,
                is_playing: false,
            },
            PreviewState::Idle => MediaControlPlayback::Library,
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
        position_ms: i64,
        /// User-facing track duration, paired with
        /// `position_ms` so consumers don't re-derive it from track state;
        /// `None` when the track's length is unknown.
        duration_ms: Option<u64>,
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
    /// The active renderer changed: playback moved to a remote renderer
    /// (Cast, DLNA, or AirPlay), returned to local output on a user stop, or
    /// returned because the link to the device was lost, with why. The UI
    /// reflects the speaker button's active state, the picker's active device,
    /// the "Playing on `<name>`" row, and the lost device's reason from this.
    RemoteStatusChanged {
        remote: RemotePlayback,
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
        position_ms: i64,
        /// User-facing track duration, paired with
        /// `position_ms` so consumers don't re-derive it from track state;
        /// `None` when the track's length is unknown.
        duration_ms: Option<u64>,
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

    fn preview_target(path: &str) -> crate::playback::PreviewTarget {
        crate::playback::PreviewTarget::sample_range(path.into(), 0, None)
    }

    #[test]
    fn media_control_uses_preview_only_while_preview_is_active() {
        let library = PlaybackValues::initial();
        assert_eq!(library.media_control(), MediaControlPlayback::Library);

        let preview = library
            .applying(&PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing {
                    target: preview_target("/tmp/preview.flac"),
                    duration_ms: 120_000,
                },
            ))
            .expect("preview state is retained");
        assert!(matches!(
            preview.media_control(),
            MediaControlPlayback::Preview {
                duration_ms: 120_000,
                is_playing: true,
                ..
            }
        ));

        let idle = preview
            .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Idle))
            .expect("idle state is retained");
        assert_eq!(idle.media_control(), MediaControlPlayback::Library);
    }

    #[test]
    fn media_control_clamps_pregap_while_ui_position_remains_signed() {
        let positions = PlaybackPositions::initial()
            .applying(&PlaybackProgress::PositionUpdate {
                position_ms: -1_250,
                duration_ms: Some(100_000),
                track_id: "track-1".to_string(),
                progress: 0.0,
            })
            .expect("position update is retained");

        assert_eq!(
            positions.library.as_ref().expect("UI position").position_ms,
            -1_250
        );
        let position = positions.media_control().expect("media-control position");
        assert_eq!(position.position_ms, 0);
        assert_eq!(position.duration_ms, Some(100_000));
    }

    #[test]
    fn position_ticks_change_no_playback_value() {
        let values = PlaybackValues::initial();
        for tick in [
            PlaybackProgress::PositionUpdate {
                position_ms: 1_000,
                duration_ms: Some(100_000),
                track_id: "track-1".to_string(),
                progress: 0.01,
            },
            PlaybackProgress::Seeked {
                position_ms: 50_000,
                duration_ms: Some(100_000),
                track_id: "track-1".to_string(),
                progress: 0.5,
            },
            PlaybackProgress::PreviewPositionUpdate {
                position_ms: 1_000,
                progress: 0.01,
            },
        ] {
            assert!(values.applying(&tick).is_none(), "{tick:?} is a position");
        }
    }

    #[test]
    fn a_state_on_another_track_ends_the_library_position() {
        let positions = PlaybackPositions::initial()
            .applying(&PlaybackProgress::PositionUpdate {
                position_ms: 1_000,
                duration_ms: Some(100_000),
                track_id: "track-1".to_string(),
                progress: 0.01,
            })
            .expect("position update is retained");
        let playing = |track_id: &str| PlaybackProgress::StateChanged {
            state: PlaybackState::Playing {
                track: crate::playback::PlayingTrack {
                    track_id: track_id.to_string(),
                    duration_ms: Some(100_000),
                },
            },
        };

        let same_track = positions
            .applying(&playing("track-1"))
            .expect("a state change is folded");
        assert_eq!(same_track, positions);

        let next_track = positions
            .applying(&playing("track-2"))
            .expect("a state change is folded");
        assert_eq!(next_track.library, None);

        let stopped = positions
            .applying(&PlaybackProgress::StateChanged {
                state: PlaybackState::Stopped,
            })
            .expect("a state change is folded");
        assert_eq!(stopped.library, None);
    }

    #[test]
    fn a_preview_position_names_its_clip_and_keeps_its_place_only_on_that_clip() {
        let first = preview_target("/tmp/first.flac");
        let playing_first = PlaybackPositions::initial()
            .applying(&PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing {
                    target: first.clone(),
                    duration_ms: 120_000,
                },
            ))
            .and_then(|positions| {
                positions.applying(&PlaybackProgress::PreviewPositionUpdate {
                    position_ms: 30_000,
                    progress: 0.25,
                })
            })
            .expect("the first clip plays");
        assert_eq!(
            playing_first.preview,
            Some(PreviewPosition {
                target: first.clone(),
                duration_ms: 120_000,
                position_ms: 30_000,
                progress: 0.25,
            })
        );

        let paused_first = playing_first
            .applying(&PlaybackProgress::PreviewStateChanged(
                PreviewState::Paused {
                    target: first,
                    duration_ms: 120_000,
                },
            ))
            .expect("the first clip pauses");
        assert_eq!(paused_first, playing_first);

        let second = preview_target("/tmp/second.flac");
        let playing_second = paused_first
            .applying(&PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing {
                    target: second.clone(),
                    duration_ms: 60_000,
                },
            ))
            .expect("the second clip plays");
        assert_eq!(
            playing_second.preview,
            Some(PreviewPosition {
                target: second,
                duration_ms: 60_000,
                position_ms: 0,
                progress: 0.0,
            })
        );

        let idle = playing_second
            .applying(&PlaybackProgress::PreviewStateChanged(PreviewState::Idle))
            .expect("the preview ends");
        assert_eq!(idle.preview, None);
        assert!(
            idle.applying(&PlaybackProgress::PreviewPositionUpdate {
                position_ms: 1_000,
                progress: 0.01,
            })
            .is_none(),
            "a position with no preview loaded is no one's"
        );
    }
}
