//! The explicit playback slot state machine.
//!
//! `PlaybackSlot` is the service thread's single authority for what is playing
//! and in what phase: `Stopped`, `Loading` (resolving a fresh load), or `Active`
//! with a `CurrentTrack` in a definite `TrackPhase`. Modeling every combination
//! as a variant keeps the illegal ones unrepresentable. The shared `AudioState`
//! atomic is a projection of this (written by `PlaybackService::sync_audio_state`),
//! never read back as truth.

use super::*;

/// Monotonic id for one decoder load (a fresh play or a seek). A track can be
/// (re)loaded under the same track id — RepeatCurrent, RestartCurrent, and
/// re-Play all replay the same id through a fresh load — so load identity is
/// per-load, not per-track. Minted from a counter owned by `PlaybackService`; a
/// `TrackReady` carrying a generation that no longer matches the current load
/// belongs to an abandoned one and is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoadGeneration(pub(super) u64);

/// The paused sub-state of a current track. `Manual` is a user pause;
/// `SideEnded` is the automatic pause at a physical-side boundary, holding the
/// `SidePauseDecision` for the track it resumes into. The public
/// `PlaybackPauseReason` surfaces only the prompt; the resume target stays
/// core-internal.
#[derive(Debug, Clone)]
pub(super) enum PausePhase {
    Manual,
    SideEnded(SidePauseDecision),
}

impl PausePhase {
    pub(super) fn to_reason(&self) -> PlaybackPauseReason {
        match self {
            PausePhase::Manual => PlaybackPauseReason::Manual,
            PausePhase::SideEnded(decision) => {
                PlaybackPauseReason::SideEnded(decision.prompt.clone())
            }
        }
    }
}

/// Where a starting load lands once its audio is ready. The value is absolute
/// (set-state-don't-toggle): a caller that wants the incoming track paused
/// computes this from the outgoing track's play intent before teardown.
#[derive(Debug, Clone)]
pub(super) enum PlayTarget {
    Playing,
    Paused(PausePhase),
}

impl PlayTarget {
    pub(super) fn into_track_phase(self) -> TrackPhase {
        match self {
            PlayTarget::Playing => TrackPhase::Playing,
            PlayTarget::Paused(pause) => TrackPhase::Paused(pause),
        }
    }
}

/// The play/pause/loading phase of the current track. This — not the shared
/// `AudioState` atomic — is the service's authority; the atomic is written as a
/// projection of it.
pub(super) enum TrackPhase {
    /// The decoder ring isn't confirmed full yet; a `TrackReady { generation }`
    /// with a matching generation resolves the track to `target`.
    Loading {
        generation: LoadGeneration,
        target: PlayTarget,
    },
    Playing,
    Paused(PausePhase),
    /// The track drained naturally: the audio callback set the atomic to Stopped
    /// and the completion event was handled, but the stream/source/decoder are
    /// retained because AutoAdvance and the side-pause decision still read them.
    Completed,
}

/// The play/pause intent a phase projects — onto the `AudioState` atomic
/// (`sync_audio_state`) and onto the toggle/preview/skip decisions. A load
/// carries its target's intent; `Completed` is silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayIntent {
    Playing,
    Paused,
    Stopped,
}

impl TrackPhase {
    pub(super) fn intent(&self) -> PlayIntent {
        match self {
            TrackPhase::Playing
            | TrackPhase::Loading {
                target: PlayTarget::Playing,
                ..
            } => PlayIntent::Playing,
            TrackPhase::Paused(_)
            | TrackPhase::Loading {
                target: PlayTarget::Paused(_),
                ..
            } => PlayIntent::Paused,
            TrackPhase::Completed => PlayIntent::Stopped,
        }
    }
}

/// The decoder feeding the persistent output's current track: the thread and its
/// AVIO cancel token. The stream, source, and audio-events receiver are not here
/// — they live in `PlaybackService::output`, shared across tracks. A track
/// transition cancels this token (stops the outgoing AVIO reads) and installs the
/// incoming decoder; the source-side swap happens via `PlaybackSource::replace`.
pub(super) struct TrackDecoder {
    pub(super) handle: std::thread::JoinHandle<()>,
    pub(super) cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

/// Everything that exists exactly when a track is current, held as one
/// always-consistent whole: the prepared track, its decoder, and its phase. The
/// output stream / source / audio-events receiver are not per-track — they live
/// in `PlaybackService::output` and persist across track transitions.
pub(super) struct CurrentTrack {
    pub(super) prepared: PlaybackPreparedTrack,
    pub(super) decoder: TrackDecoder,
    pub(super) phase: TrackPhase,
}

pub(super) enum PlaybackSlot {
    Stopped,
    /// `play_track` has torn down the old track and is resolving the new one.
    /// `resolved` is None before the metadata lookup and Some after — the two
    /// Loading emissions the UI contract already expects.
    Loading {
        track_id: String,
        resolved: Option<LoadingTrack>,
    },
    Active(CurrentTrack),
}

impl PlaybackSlot {
    /// The current track's id, once one exists (Active in any phase). None while
    /// the slot is Stopped or still resolving a fresh load.
    pub(super) fn current_track_id(&self) -> Option<&str> {
        match self {
            PlaybackSlot::Active(cur) => Some(cur.prepared.track_info.track_id.as_str()),
            _ => None,
        }
    }
}
