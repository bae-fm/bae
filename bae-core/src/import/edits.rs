//! What a person typed over a candidate's metadata, and what became of an
//! import that failed.
//!
//! Every one of these is written the moment it happens and read back by the
//! per-candidate query, so the pane holds no copy of its own: the control
//! writes, the query redraws.

use crate::import::types::{AudioFile, RawTrackEdit};
use chrono::{DateTime, Utc};

/// What a person did to one row of the mapping table.
///
/// A track row is edited as a unit — the control hands back the whole row —
/// so the stored value is the whole row rather than a per-field overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateTrackEdit {
    pub track_id: String,
    pub state: TrackEditState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackEditState {
    /// The row is out of the import: the release commits without that track.
    Dropped,
    /// The row as the person left it.
    Edited(RawTrackEdit),
}

impl CandidateTrackEdit {
    pub fn dropped(track_id: impl Into<String>) -> Self {
        Self {
            track_id: track_id.into(),
            state: TrackEditState::Dropped,
        }
    }

    pub fn edited(edit: RawTrackEdit) -> Self {
        Self {
            track_id: edit.id.clone(),
            state: TrackEditState::Edited(edit),
        }
    }

    /// The audio the edited row is pointed at, where it names any.
    pub fn file(&self) -> Option<&AudioFile> {
        match &self.state {
            TrackEditState::Dropped => None,
            TrackEditState::Edited(edit) => edit.file.as_ref(),
        }
    }
}

/// The last import of this candidate that failed, as the pane still shows it
/// after a relaunch. An artist identity conflict carries the two library rows
/// the pane can offer to consolidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub error: String,
    pub failed_at: DateTime<Utc>,
    pub artist_identity_conflict: Option<crate::import::ArtistIdentityConflict>,
}

#[cfg(test)]
impl ImportFailure {
    pub(crate) fn error_only(error: impl Into<String>, failed_at: DateTime<Utc>) -> Self {
        Self {
            error: error.into(),
            failed_at,
            artist_identity_conflict: None,
        }
    }
}
