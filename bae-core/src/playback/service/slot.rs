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

/// Everything that exists exactly when a track is current, held as one
/// always-consistent whole. The stream + source + decoder are the shared
/// `StreamPipeline`; the audio-events receiver stays a `CurrentTrack` field
/// (not inside the pipeline) because the service drains it on its select tick,
/// while preview moves its receiver into a spawned task — that asymmetry lives
/// at the owner, not in the shared unit.
pub(super) struct CurrentTrack {
    pub(super) prepared: PlaybackPreparedTrack,
    pub(super) pipeline: StreamPipeline,
    pub(super) audio_events: AudioEventReceiver,
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

    /// The current track while it is actively streaming — Active and not yet
    /// Completed. A Completed track keeps its receiver but stops being polled:
    /// nothing reads its counters again, and the next stream replaces it whole.
    fn streaming(&self) -> Option<&CurrentTrack> {
        match self {
            PlaybackSlot::Active(cur) if !matches!(cur.phase, TrackPhase::Completed) => Some(cur),
            _ => None,
        }
    }

    fn streaming_mut(&mut self) -> Option<&mut CurrentTrack> {
        match self {
            PlaybackSlot::Active(cur) if !matches!(cur.phase, TrackPhase::Completed) => Some(cur),
            _ => None,
        }
    }

    /// The audio-events receiver to poll on the drain tick, present only while a
    /// track is actively streaming.
    pub(super) fn pollable_audio_events(&mut self) -> Option<&mut AudioEventReceiver> {
        self.streaming_mut().map(|cur| &mut cur.audio_events)
    }

    pub(super) fn pollable_audio_events_ref(&self) -> Option<&AudioEventReceiver> {
        self.streaming().map(|cur| &cur.audio_events)
    }

    pub(super) fn has_pollable_audio_events(&self) -> bool {
        self.streaming().is_some()
    }
}
