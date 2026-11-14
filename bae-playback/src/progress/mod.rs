pub mod anchor;
pub mod handle;
pub mod timeline;
use crate::renderer::CastReceiver;
use crate::service::PlaybackState;
use crate::timeline::{StreamPosition, TrackTimeline};
pub use anchor::{AnchorFold, TrackAnchor};
pub use handle::{PlaybackPositionsWatch, PlaybackProgressFeed, PlaybackProgressHandle};
pub use timeline::{PlaybackTimeline, PreviewTimelinePoint, TimelinePoint};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::warn;

/// Display-ready state of preview (source-window audition) playback. Carries
/// identity and duration only — its position is [`PreviewPosition`], which
/// moves while the preview plays.
#[derive(Debug, Clone, PartialEq)]
pub enum PreviewState {
    Idle,
    Playing { target: bae_track_audio::PreviewTarget, duration_ms: u64 },
    Paused { target: bae_track_audio::PreviewTarget, duration_ms: u64 },
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
    /// How fast the position moves while the track plays, as a multiple of
    /// real time: 1.0 for this device's own output, a remote renderer's
    /// stated rate, or 0.0 for one that states none (its position stands
    /// until its next status).
    pub rate: f64,
}

/// Where the preview is in its clip, and which clip that is, so a surface
/// never shows one preview's position against another preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewPosition {
    pub target: bae_track_audio::PreviewTarget,
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
        target: bae_track_audio::PreviewTarget,
        duration_ms: u64,
        is_playing: bool,
    },
}

/// What playback is doing, apart from where it is in what it plays: these
/// change when playback does something, while [`PlaybackPositions`] move
/// as audio plays. `Track` is what the state holds of the playing track; see
/// [`PlaybackState`].
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackValues<Track = crate::PlayingTrack> {
    pub state: PlaybackState<Track>,
    pub volume: f32,
    pub is_muted: bool,
    /// Where playback is going: this device's own output, a remote renderer,
    /// or back here after the link to a renderer was lost.
    pub remote: RemotePlayback,
    pub preview: PreviewState,
}

/// One change to what playback is doing, as the device's `playback_now` row
/// takes it: a value that changed, or a failure.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackChange {
    State(PlaybackState),
    Volume(f32),
    Muted(bool),
    Remote(RemotePlayback),
    Preview(PreviewState),
    /// Playback or a preview failed; the row holds why until playback next
    /// plays.
    Failed(crate::PlaybackErrorReason),
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
    /// Whether the library track is playing, so its position moves; false
    /// while it is paused, loading or stopped.
    pub playing: bool,
    /// `None` while no preview is loaded.
    pub preview: Option<PreviewPosition>,
    /// Monotonic acknowledgement of an applied preview seek, kept like
    /// [`Self::seek_revision`] across ordinary preview ticks and across clips.
    pub preview_seek_revision: u64,
}

/// Where playback is going, for the cast surfaces: the speaker button's state,
/// the picker's active device, and why the last device stopped playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemotePlayback {
    /// This device's own output.
    Local,
    /// A remote renderer (Cast, DLNA or AirPlay).
    On(CastReceiver),
    /// Back on this device's own output because the link to `device` was lost:
    /// the receiver stopped its app, went unreachable, or the network dropped.
    /// `detail` is why, untranslated. Held until playback moves to a device
    /// again, so the picker can say what happened to the last one.
    Ended { device: CastReceiver, detail: String },
}

impl PlaybackValues {
    /// What playback does as it starts: stopped, at full volume, unmuted,
    /// on this device's output, previewing nothing. The `playback_now` row
    /// starting playback leaves holds these, apart from a resumed volume
    /// and mute, which playback announces as it resumes.
    pub(super) fn initial() -> Self {
        Self {
            state: PlaybackState::Stopped,
            volume: 1.0,
            is_muted: false,
            remote: RemotePlayback::Local,
            preview: PreviewState::Idle,
        }
    }

    /// What `event` asks the row to take: a value it moves, or the failure it
    /// reports. `None` when it moves none of them. These values are left as
    /// they are: they follow the row, through [`Self::took`].
    pub(super) fn change_for(&self, event: &PlaybackProgress) -> Option<PlaybackChange> {
        fn moved<T: PartialEq + Clone>(
            held: &T,
            next: &T,
            change: impl FnOnce(T) -> PlaybackChange,
        ) -> Option<PlaybackChange> {
            (held != next).then(|| change(next.clone()))
        }
        match event {
            PlaybackProgress::StateChanged { state } => {
                moved(&self.state, state, PlaybackChange::State)
            }
            PlaybackProgress::VolumeChanged { volume } => {
                moved(&self.volume, volume, PlaybackChange::Volume)
            }
            PlaybackProgress::MuteChanged { is_muted } => {
                moved(&self.is_muted, is_muted, PlaybackChange::Muted)
            }
            PlaybackProgress::RemoteStatusChanged { remote } => {
                moved(&self.remote, remote, PlaybackChange::Remote)
            }
            PlaybackProgress::PreviewStateChanged(state) => {
                moved(&self.preview, state, PlaybackChange::Preview)
            }
            PlaybackProgress::PlaybackError { reason }
            | PlaybackProgress::PreviewFailed { reason } => {
                Some(PlaybackChange::Failed(reason.clone()))
            }
            PlaybackProgress::PositionUpdate { .. }
            | PlaybackProgress::Seeked { .. }
            | PlaybackProgress::PreviewPositionUpdate { .. }
            | PlaybackProgress::PreviewSeeked { .. }
            | PlaybackProgress::TrackCompleted { .. }
            | PlaybackProgress::DecodeStats { .. }
            | PlaybackProgress::ServiceEnded => None,
        }
    }

    /// The row took `change`: these values now hold it. A failure is not one
    /// of the values.
    pub(super) fn took(&mut self, change: &PlaybackChange) {
        match change {
            PlaybackChange::State(state) => self.state = state.clone(),
            PlaybackChange::Volume(volume) => self.volume = *volume,
            PlaybackChange::Muted(is_muted) => self.is_muted = *is_muted,
            PlaybackChange::Remote(remote) => self.remote = remote.clone(),
            PlaybackChange::Preview(preview) => self.preview = preview.clone(),
            PlaybackChange::Failed(_) => {}
        }
    }
}

impl PlaybackPositions {
    pub(super) fn initial() -> Self {
        Self {
            library: None,
            seek_revision: 0,
            playing: false,
            preview: None,
            preview_seek_revision: 0,
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
                next.playing = matches!(state, PlaybackState::Playing { .. });
            }
            PlaybackProgress::PositionUpdate {
                track_id,
                position_ms,
                duration_ms,
                progress,
                rate,
            } => {
                next.library = Some(PlaybackPosition {
                    track_id: track_id.clone(),
                    position_ms: *position_ms,
                    duration_ms: *duration_ms,
                    progress: *progress,
                    rate: *rate,
                });
            }
            PlaybackProgress::Seeked { track_id, position_ms, duration_ms, progress, rate } => {
                next.library = Some(PlaybackPosition {
                    track_id: track_id.clone(),
                    position_ms: *position_ms,
                    duration_ms: *duration_ms,
                    progress: *progress,
                    rate: *rate,
                });
                next.seek_revision =
                    next.seek_revision.checked_add(1).expect("playback seek revision overflow");
            }
            PlaybackProgress::PreviewStateChanged(PreviewState::Idle) => next.preview = None,
            PlaybackProgress::PreviewStateChanged(
                PreviewState::Playing { target, duration_ms }
                | PreviewState::Paused { target, duration_ms },
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
            PlaybackProgress::PreviewPositionUpdate { position_ms, progress } => {
                let Some(preview) = next.preview.as_mut() else {
                    tracing::debug!(
                        "preview position {position_ms} ms arrived with no preview loaded; dropped"
                    );
                    return None;
                };
                preview.position_ms = *position_ms;
                preview.progress = *progress;
            }
            PlaybackProgress::PreviewSeeked { position_ms, progress } => {
                let Some(preview) = next.preview.as_mut() else {
                    tracing::debug!(
                        "preview seek to {position_ms} ms arrived with no preview loaded; dropped"
                    );
                    return None;
                };
                preview.position_ms = *position_ms;
                preview.progress = *progress;
                next.preview_seek_revision = next
                    .preview_seek_revision
                    .checked_add(1)
                    .expect("preview seek revision overflow");
            }
            PlaybackProgress::VolumeChanged { .. }
            | PlaybackProgress::MuteChanged { .. }
            | PlaybackProgress::RemoteStatusChanged { .. }
            | PlaybackProgress::PlaybackError { .. }
            | PlaybackProgress::PreviewFailed { .. }
            | PlaybackProgress::TrackCompleted { .. }
            | PlaybackProgress::DecodeStats { .. }
            | PlaybackProgress::ServiceEnded => return None,
        }
        Some(next)
    }
}

impl<Track> PlaybackValues<Track> {
    /// The same values holding `f`'s result for the state's track, or `f`'s
    /// error.
    pub fn try_map_track<Mapped, Error>(
        self,
        f: impl FnMut(Track) -> Result<Mapped, Error>,
    ) -> Result<PlaybackValues<Mapped>, Error> {
        let Self { state, volume, is_muted, remote, preview } = self;
        Ok(PlaybackValues { state: state.try_map_track(f)?, volume, is_muted, remote, preview })
    }

    /// What the operating-system media surface shows of these values.
    pub fn media_control(&self) -> MediaControlPlayback {
        match &self.preview {
            PreviewState::Playing { target, duration_ms } => MediaControlPlayback::Preview {
                target: target.clone(),
                duration_ms: *duration_ms,
                is_playing: true,
            },
            PreviewState::Paused { target, duration_ms } => MediaControlPlayback::Preview {
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
        /// How fast the position moves while the track plays; see
        /// [`PlaybackPosition::rate`].
        rate: f64,
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
        reason: crate::PlaybackErrorReason,
    },
    /// A preview failed, before it started or as it played. The preview ends;
    /// the main player is not the preview's to halt, so it plays on. Screens
    /// show the reason as they show a playback error.
    PreviewFailed {
        reason: crate::PlaybackErrorReason,
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
        /// How fast the position moves while the track plays; see
        /// [`PlaybackPosition::rate`].
        rate: f64,
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
    /// The preview's stream was rebuilt at a new position, by a seek: the
    /// position moved, and the seek is applied.
    PreviewSeeked {
        position_ms: u64,
        progress: f64,
    },
    /// A service ended and its end has been told: nothing of it follows, and
    /// whatever comes next is another service's.
    ServiceEnded,
}

impl PlaybackProgress {
    /// The playing tick at `position` of a track on `timeline`, moving at
    /// `rate`.
    pub(crate) fn position_update(
        timeline: TrackTimeline,
        track_id: String,
        position: StreamPosition,
        rate: f64,
    ) -> Self {
        Self::PositionUpdate {
            position_ms: timeline.track_time(position).as_millis(),
            duration_ms: timeline.duration_ms(),
            track_id,
            progress: timeline.progress(position),
            rate,
        }
    }

    /// The display moved to `position` of a track on `timeline` without a
    /// state change, moving on at `rate`.
    pub(crate) fn seeked(
        timeline: TrackTimeline,
        track_id: String,
        position: StreamPosition,
        rate: f64,
    ) -> Self {
        Self::Seeked {
            position_ms: timeline.track_time(position).as_millis(),
            duration_ms: timeline.duration_ms(),
            track_id,
            progress: timeline.progress(position),
            rate,
        }
    }

    /// The preview tick at `position` of a preview on `timeline`.
    pub(crate) fn preview_position(timeline: TrackTimeline, position: StreamPosition) -> Self {
        Self::PreviewPositionUpdate {
            position_ms: position.as_millis(),
            progress: timeline.progress(position),
        }
    }

    /// The preview's stream rebuilt at `position` of a preview on `timeline`.
    pub(crate) fn preview_seeked(timeline: TrackTimeline, position: StreamPosition) -> Self {
        Self::PreviewSeeked {
            position_ms: position.as_millis(),
            progress: timeline.progress(position),
        }
    }
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
#[path = "mod_tests.rs"]
mod tests;
