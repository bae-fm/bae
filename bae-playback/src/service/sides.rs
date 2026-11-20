//! The sides and discs of a release, as playback pauses between them: which
//! side a track is on, the boundary between two tracks, and the prompt a pause
//! there shows.

/// What the playback service decides a side or disc pause with about a track:
/// the release and side it plays on, read from the library when the decision
/// is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackInfo {
    pub track_id: String,
    pub release_id: String,
    /// The side or disc the track plays on, as its heading names it, for a
    /// track on a physical carrier; `None` for any other.
    pub side: Option<bae_types::mediums::TrackSide>,
}

impl PlaybackTrackInfo {
    /// `track_id`'s facts as the library holds them now; `None` when it
    /// holds no such track.
    pub async fn read(
        records: &bae_records::Records,
        track_id: &str,
    ) -> Result<Option<Self>, bae_store::DbError> {
        Ok(records.track_places(&[track_id.to_string()]).await?.into_iter().next().map(Self::of))
    }

    /// The facts of the track at `place`. A track on a medium with no
    /// physical carrier (a download, a disc of unstated carrier) is on no
    /// side playback pauses at.
    pub(crate) fn of(place: bae_records::DbTrackPlace) -> Self {
        let side = place.carrier.and_then(bae_types::pressing::Medium::physical).map(|_| place.on);
        Self { track_id: place.track_id, release_id: place.release_id, side }
    }
}

/// What the side/disc pause prompt shows: the heading of the side or disc
/// that ended, which its title names and whose kind words its other lines,
/// and whether the next side starts on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSidePausePrompt {
    pub id: String,
    pub ended: bae_types::mediums::SideHeading,
    /// The countdown to the next side starting on its own, or `None` when the
    /// pause waits for Play.
    pub countdown: Option<PlaybackSideCountdown>,
}

/// A running side-pause countdown. Every UI counts down to `resumes_at`, so
/// they all show the same number; core decides when the next side starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSideCountdown {
    pub resumes_at: time::UtcDateTime,
}

/// The side or disc boundary between two tracks: the pause's id and the
/// heading of the side or disc that ends there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SideBoundary {
    pub(super) id: String,
    pub(super) ended: bae_types::mediums::SideHeading,
}

/// A pause at a side boundary, resuming into `track_id` on Play or, when set,
/// on its own at `resumes_at`.
#[derive(Debug, Clone)]
pub(super) struct SidePauseDecision {
    pub(super) track_id: String,
    pub(super) boundary: SideBoundary,
    pub(super) resumes_at: Option<time::UtcDateTime>,
}

impl SidePauseDecision {
    pub(super) fn prompt(&self) -> PlaybackSidePausePrompt {
        let SideBoundary { id, ended } = &self.boundary;
        PlaybackSidePausePrompt {
            id: id.clone(),
            ended: ended.clone(),
            countdown: self.resumes_at.map(|resumes_at| PlaybackSideCountdown { resumes_at }),
        }
    }
}

/// The side or disc boundary between `current` and `next`: where the heading
/// of the side changes between two tracks of one release on physical
/// carriers. `None` when they play under one heading, aren't on one
/// release's sides at all, or `current` is on a flat side, which has no
/// heading to name.
pub(super) fn side_boundary_between(
    current: &PlaybackTrackInfo,
    next: &PlaybackTrackInfo,
) -> Option<SideBoundary> {
    use bae_types::mediums::SideHeading;
    if current.release_id != next.release_id {
        return None;
    }
    let current_side = current.side.as_ref()?;
    let next_side = next.side.as_ref()?;
    if current_side == next_side {
        return None;
    }
    let ended = current_side.heading()?;
    let named = match &ended {
        SideHeading::Sided { side_letter } => format!("side {side_letter}"),
        SideHeading::Disc { disc } => format!("disc {disc}"),
    };
    Some(SideBoundary { id: format!("{}:{named}", next.track_id), ended })
}

#[cfg(test)]
#[path = "sides_tests.rs"]
mod tests;
