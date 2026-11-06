//! What a play command did, as the app that sent it is told.

/// What a play command did. Mirrors `bae_playback::PlayOutcome`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePlayOutcome {
    /// Playing everything the play named. Nothing to show.
    Playing,
    /// Playing, without the releases named that hold no tracks, in the order
    /// named. The line takes how many.
    PlayingWithoutReleases { release_ids: Vec<String> },
    /// Playing, without the tracks of the list that the library doesn't hold,
    /// in the list's order. The line takes how many.
    PlayingWithoutTracks { track_ids: Vec<String> },
    /// What the play named holds no tracks.
    NothingToPlay,
    /// The track to start from is gone.
    StartMissing,
}

impl BridgePlayOutcome {
    pub(crate) fn from_core(outcome: bae_playback::PlayOutcome) -> Self {
        match outcome {
            bae_playback::PlayOutcome::Playing => Self::Playing,
            bae_playback::PlayOutcome::PlayingWithoutReleases(release_ids) => {
                Self::PlayingWithoutReleases { release_ids }
            }
            bae_playback::PlayOutcome::PlayingWithoutTracks(track_ids) => {
                Self::PlayingWithoutTracks { track_ids }
            }
            bae_playback::PlayOutcome::NothingToPlay => Self::NothingToPlay,
            bae_playback::PlayOutcome::StartMissing => Self::StartMissing,
        }
    }
}

/// The line a play's outcome shows, or `None` when it played everything it
/// named, which shows nothing. The two "without" lines take a `count`.
#[uniffi::export]
pub fn bridge_play_outcome_key(outcome: &BridgePlayOutcome) -> Option<String> {
    match outcome {
        BridgePlayOutcome::Playing => None,
        BridgePlayOutcome::PlayingWithoutReleases { .. } => {
            Some("core.play.left_out_releases".to_string())
        }
        BridgePlayOutcome::PlayingWithoutTracks { .. } => {
            Some("core.play.left_out_tracks".to_string())
        }
        BridgePlayOutcome::NothingToPlay => Some("core.play.nothing_to_play".to_string()),
        BridgePlayOutcome::StartMissing => Some("core.play.start_missing".to_string()),
    }
}

/// A queue command that changed nothing, in the class core gave it. A
/// database failure crosses as its own text, as every database failure does.
impl From<bae_playback::QueueEditError> for super::BridgeError {
    fn from(error: bae_playback::QueueEditError) -> Self {
        match error {
            bae_playback::QueueEditError::Queue(error) => error.into(),
            error => Self::categorized(error.category(), error),
        }
    }
}

/// A play with no outcome to tell: the queue couldn't be filled, a database
/// failure, or the playback service had stopped, which is a bug of whoever
/// still sent it a play.
impl From<bae_playback::PlayError> for super::BridgeError {
    fn from(error: bae_playback::PlayError) -> Self {
        match error {
            bae_playback::PlayError::Queue(error) => error.into(),
            stopped @ bae_playback::PlayError::Stopped(_) => {
                Self::diagnostic(super::BridgeErrorCategory::Internal, stopped)
            }
        }
    }
}
