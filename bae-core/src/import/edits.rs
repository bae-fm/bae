//! What a person typed over a candidate's metadata, and what became of an
//! import that failed.
//!
//! Every one of these is written the moment it happens and read back by the
//! per-candidate query, so the pane holds no copy of its own: the control
//! writes, the query redraws.

use crate::import::mapping::{
    mapping_with_track, mapping_without_track, MappingTable, MappingTrackSection,
};
use crate::import::types::{
    ArtistAssignment, AudioFile, CandidateTrack, RawReleaseEdit, RawTrackEdit, TrackFileAuthor,
};
use chrono::{DateTime, Utc};

/// One album-level field of the metadata form.
///
/// The form's own fields, not the wire edit's: `year` is text here because the
/// field is text, and the commit is what parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Format,
    Label,
    CatalogNumber,
    Country,
    Barcode,
}

impl CandidateEditField {
    /// Put `value` in this field of `draft`.
    pub(crate) fn set<Track>(
        self,
        draft: &mut crate::import::RawReleaseEditOf<Track>,
        value: &str,
    ) {
        let slot = match self {
            Self::AlbumTitle => &mut draft.album_title,
            Self::AlbumYear => &mut draft.album_year,
            Self::PressingYear => &mut draft.pressing.year,
            Self::Format => &mut draft.pressing.format,
            Self::Label => &mut draft.pressing.label,
            Self::CatalogNumber => &mut draft.pressing.catalog_number,
            Self::Country => &mut draft.pressing.country,
            Self::Barcode => &mut draft.pressing.barcode,
        };
        *slot = value.to_string();
    }
}

/// The album-level fields a person has typed, over whatever the picked release
/// seeds.
///
/// Per field, not per form: `None` is "nobody touched this", so the commit
/// still reads it as untouched and does not clear what the release states. A
/// stored string — the empty one included — is the person's value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateEditOverlay {
    pub album_title: Option<String>,
    pub album_artist_assignments: Option<Vec<ArtistAssignment>>,
    pub album_year: Option<String>,
    pub pressing_year: Option<String>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

impl CandidateEditOverlay {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `seed` with every field the person typed replaced by what they typed.
    pub fn apply(&self, mut seed: RawReleaseEdit) -> RawReleaseEdit {
        let overwrite = |target: &mut String, value: &Option<String>| {
            if let Some(value) = value {
                target.clone_from(value);
            }
        };
        overwrite(&mut seed.album_title, &self.album_title);
        if let Some(assignments) = &self.album_artist_assignments {
            seed.album_artist_assignments.clone_from(assignments);
        }
        overwrite(&mut seed.album_year, &self.album_year);
        overwrite(&mut seed.pressing.year, &self.pressing_year);
        overwrite(&mut seed.pressing.format, &self.format);
        overwrite(&mut seed.pressing.label, &self.label);
        overwrite(&mut seed.pressing.catalog_number, &self.catalog_number);
        overwrite(&mut seed.pressing.country, &self.country);
        overwrite(&mut seed.pressing.barcode, &self.barcode);
        seed
    }
}

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

/// Lay every stored row edit over a freshly projected table.
///
/// The same two operations the pane used to run on its own copy, now run where
/// the table is built — so a reopened pane shows what the person left, and the
/// commit consumes it without a second path.
pub fn apply_track_edits(table: MappingTable, edits: &[CandidateTrackEdit]) -> MappingTable {
    edits.iter().fold(table, |table, edit| match &edit.state {
        TrackEditState::Dropped => mapping_without_track(table, &edit.track_id),
        TrackEditState::Edited(track) => mapping_with_track(table, track.clone()),
    })
}

/// Lay the stored rows' decisions over a freshly projected table: a dropped
/// row leaves it, and every other row takes the file its stored row names.
pub(crate) fn apply_track_decisions(
    table: MappingTable,
    tracks: &[CandidateTrack],
) -> MappingTable {
    tracks.iter().fold(table, |table, stored| {
        if stored.dropped {
            return mapping_without_track(table, &stored.edit.id);
        }
        let track = table
            .track_sections
            .iter()
            .flat_map(MappingTrackSection::mappings)
            .find_map(|mapping| match &mapping.becomes {
                crate::import::mapping::MappingBecomes::Track { track, .. }
                    if track.id == stored.edit.id =>
                {
                    Some(track.clone())
                }
                _ => None,
            });
        match track {
            Some(mut track) => {
                track.file.clone_from(&stored.edit.file);
                mapping_with_track(table, track)
            }
            None => table,
        }
    })
}

/// Carry user-owned file bindings and dropped rows onto a newly projected
/// metadata source. Automatic bindings and source membership come from the
/// new projection.
pub(crate) fn preserve_track_decisions(
    proposed: Vec<CandidateTrack>,
    current: &[CandidateTrack],
) -> Vec<CandidateTrack> {
    carry_track_decisions(proposed, current, |_| true)
}

/// Carry decisions onto a new candidate file shape. A user binding survives
/// only while the audio unit it names still exists; automatic bindings are the
/// new projection's answer.
pub(crate) fn reconcile_track_decisions(
    proposed: Vec<CandidateTrack>,
    current: &[CandidateTrack],
    available: &std::collections::HashSet<AudioFile>,
) -> Vec<CandidateTrack> {
    carry_track_decisions(proposed, current, |file| {
        file.as_ref().is_none_or(|file| available.contains(file))
    })
}

fn carry_track_decisions(
    mut proposed: Vec<CandidateTrack>,
    current: &[CandidateTrack],
    keep_user_file: impl Fn(&Option<AudioFile>) -> bool,
) -> Vec<CandidateTrack> {
    let current: std::collections::HashMap<_, _> = current
        .iter()
        .map(|track| (track.edit.id.as_str(), track))
        .collect();
    for track in &mut proposed {
        let Some(existing) = current.get(track.edit.id.as_str()) else {
            continue;
        };
        if let Some(file) = existing.user_file().filter(|file| keep_user_file(file)) {
            track.edit.file.clone_from(file);
            track.file_author = TrackFileAuthor::User;
        }
        // A row out of the import plays nothing: the drop takes the file
        // with it, as the row edit that dropped it did.
        if existing.dropped {
            track.dropped = true;
            track.edit.file = None;
        }
    }
    proposed
}
