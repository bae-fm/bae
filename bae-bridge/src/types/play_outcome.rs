//! What a play command did, as the app that sent it is told.

/// What a play command did. Mirrors `bae_playback::PlayOutcome`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePlayOutcome {
    /// Playing everything the play named. Nothing to show.
    Playing,
    /// Playing, without `count` of the releases named, which hold no tracks.
    /// The line takes the count.
    PlayingWithoutReleases { count: u32 },
    /// Playing, without `count` of the tracks of the list, which the library
    /// doesn't hold. The line takes the count.
    PlayingWithoutTracks { count: u32 },
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
                tracing::warn!(?release_ids, "playing without releases that hold no tracks");
                Self::PlayingWithoutReleases { count: count(&release_ids) }
            }
            bae_playback::PlayOutcome::PlayingWithoutTracks(track_ids) => {
                tracing::warn!(?track_ids, "playing without tracks the library lacks");
                Self::PlayingWithoutTracks { count: count(&track_ids) }
            }
            bae_playback::PlayOutcome::NothingToPlay => Self::NothingToPlay,
            bae_playback::PlayOutcome::StartMissing => Self::StartMissing,
        }
    }
}

fn count(ids: &[String]) -> u32 {
    u32::try_from(ids.len()).expect("a play names fewer than 2^32 ids")
}

/// The line a play's outcome shows, or `None` when it played everything it
/// named, which shows nothing. The two "without" lines take their `count`.
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
/// failure, or the playback service had stopped or ended, which is a bug: of
/// whoever still sent it a play, or the one its thread ended on.
impl From<bae_playback::PlayError> for super::BridgeError {
    fn from(error: bae_playback::PlayError) -> Self {
        match error {
            bae_playback::PlayError::Queue(error) => error.into(),
            gone @ (bae_playback::PlayError::Stopped(_) | bae_playback::PlayError::Ended(_)) => {
                Self::diagnostic(super::BridgeErrorCategory::Internal, gone)
            }
        }
    }
}

impl From<bae_app::PlayPlaylistError> for super::BridgeError {
    fn from(error: bae_app::PlayPlaylistError) -> Self {
        match error {
            bae_app::PlayPlaylistError::Entries(error) => error.into(),
            bae_app::PlayPlaylistError::Play(error) => error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A play that left releases or tracks out tells the apps how many, and the
    /// apps count nothing.
    #[test]
    fn a_play_that_left_out_ids_tells_how_many() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            BridgePlayOutcome::from_core(bae_playback::PlayOutcome::PlayingWithoutReleases(
                ids.clone()
            )),
            BridgePlayOutcome::PlayingWithoutReleases { count: 3 }
        );
        assert_eq!(
            BridgePlayOutcome::from_core(bae_playback::PlayOutcome::PlayingWithoutTracks(ids)),
            BridgePlayOutcome::PlayingWithoutTracks { count: 3 }
        );
    }
}
