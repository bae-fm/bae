//! The sides and discs of a release, as playback pauses between them: which
//! side a track is on, the boundary between two tracks, and the prompt a pause
//! there shows.

use super::*;

/// What the playback service decides a side or disc pause with about a track:
/// the release and side it plays on, read from the library when the decision
/// is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackInfo {
    pub track_id: String,
    pub release_id: String,
    pub side: Option<PlaybackTrackSide>,
}

impl PlaybackTrackInfo {
    /// `track_id`'s facts as the library holds them now; `None` when it
    /// holds no such track.
    pub async fn read(
        records: &bae_records::Records,
        track_id: &str,
    ) -> Result<Option<Self>, bae_store::DbError> {
        let Some(track) = records.find_track_by_id(track_id).await? else {
            return Ok(None);
        };
        let mediums = records.get_release_mediums(&track.release_id).await?;
        Ok(Some(Self::of(&track, &mediums)))
    }

    /// The facts of `track`, which sits on one of `mediums`, its release's. A
    /// track on a medium with no physical carrier — a download, a disc of
    /// unstated carrier — is on no side playback pauses at.
    pub(crate) fn of(
        track: &bae_records::DbTrack,
        mediums: &bae_records::DbReleaseMediums,
    ) -> Self {
        let place = mediums.place(track);
        let side = mediums
            .medium_of(track)
            .medium
            .carrier
            .and_then(bae_types::pressing::Medium::physical)
            .map(|physical| {
                let boundary = match physical {
                    PhysicalMedium::Record | PhysicalMedium::Cassette => {
                        PlaybackPauseBoundary::Side
                    }
                    PhysicalMedium::Cd => PlaybackPauseBoundary::Disc,
                };
                let label = match bae_types::mediums::track_side(&place, &mediums.mediums()) {
                    bae_types::mediums::TrackSide::Sided { side_letter } => side_letter,
                    bae_types::mediums::TrackSide::Disc { disc } => disc.to_string(),
                    bae_types::mediums::TrackSide::Flat => place.medium.to_string(),
                };
                PlaybackTrackSide { medium: place.medium, side: place.side, boundary, label }
            });
        Self { track_id: track.id.clone(), release_id: track.release_id.clone(), side }
    }
}

/// The side or disc a track is on, which decides where playback pauses: a
/// change of medium or of side is a boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackSide {
    /// The medium's position among its release's.
    pub medium: std::num::NonZeroU16,
    /// The side of that medium.
    pub side: std::num::NonZeroU8,
    /// What ends there: a record's or cassette's side, or a CD.
    pub boundary: PlaybackPauseBoundary,
    /// How the side reads ("B", or the disc's number).
    pub label: String,
}

/// What the side/disc pause prompt shows: which boundary playback stopped at,
/// and whether the next side starts on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSidePausePrompt {
    pub id: String,
    /// Whether a side or a disc ended; the prompt's wording follows it.
    pub boundary: PlaybackPauseBoundary,
    pub side_label: String,
    /// The countdown to the next side starting on its own, or `None` when the
    /// pause waits for Play.
    pub countdown: Option<PlaybackSideCountdown>,
}

/// The kind of boundary a pause between sides stopped at: a record's or a
/// cassette's side, or a CD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPauseBoundary {
    Side,
    Disc,
}

/// A running side-pause countdown. Every UI counts down to `resumes_at`, so
/// they all show the same number; core decides when the next side starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSideCountdown {
    pub resumes_at: chrono::DateTime<chrono::Utc>,
}

/// The side or disc boundary between two tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SideBoundary {
    pub(super) id: String,
    pub(super) kind: PlaybackPauseBoundary,
    pub(super) side_label: String,
}

/// A pause at a side boundary, resuming into `track_id` on Play or, when set,
/// on its own at `resumes_at`.
#[derive(Debug, Clone)]
pub(super) struct SidePauseDecision {
    pub(super) track_id: String,
    pub(super) boundary: SideBoundary,
    pub(super) resumes_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SidePauseDecision {
    pub(super) fn prompt(&self) -> PlaybackSidePausePrompt {
        let SideBoundary { id, kind, side_label } = &self.boundary;
        PlaybackSidePausePrompt {
            id: id.clone(),
            boundary: *kind,
            side_label: side_label.clone(),
            countdown: self.resumes_at.map(|resumes_at| PlaybackSideCountdown { resumes_at }),
        }
    }
}

/// The side or disc boundary between `current` and `next`, or `None` when they
/// play on the same side (or aren't on one release's sides at all).
pub(super) fn side_boundary_between(
    current: &PlaybackTrackInfo,
    next: &PlaybackTrackInfo,
) -> Option<SideBoundary> {
    if current.release_id != next.release_id {
        return None;
    }
    let current_side = current.side.as_ref()?;
    let next_side = next.side.as_ref()?;
    if (current_side.medium, current_side.side) == (next_side.medium, next_side.side) {
        return None;
    }
    Some(SideBoundary {
        id: format!("{}:{}:{}", next.track_id, current_side.medium, current_side.side),
        kind: current_side.boundary,
        side_label: current_side.label.clone(),
    })
}

#[cfg(test)]
#[path = "sides_tests.rs"]
mod tests;
