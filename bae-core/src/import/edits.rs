//! What a person typed over a candidate's metadata, and what became of an
//! import that failed.
//!
//! Every one of these is written the moment it happens and read back by the
//! per-candidate query, so the pane holds no copy of its own: the control
//! writes, the query redraws.

use crate::import::mapping::{mapping_with_track, mapping_without_track, MappingTable};
use crate::import::types::{ArtistAssignment, AudioFile, RawReleaseEdit, RawTrackEdit};
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
    /// The column this field is stored in.
    pub(crate) fn column(self) -> &'static str {
        match self {
            Self::AlbumTitle => "album_title",
            Self::AlbumYear => "album_year",
            Self::PressingYear => "year",
            Self::Format => "format",
            Self::Label => "label",
            Self::CatalogNumber => "catalog_number",
            Self::Country => "country",
            Self::Barcode => "barcode",
        }
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

/// A track row's physical decision, stored independently from its editable
/// metadata so replacing or clearing metadata cannot delete the pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateTrackMappingEdit {
    pub track_id: String,
    pub dropped: bool,
    pub file: Option<AudioFile>,
}

impl CandidateTrackMappingEdit {
    pub(crate) fn from_track_edit(edit: &CandidateTrackEdit) -> Self {
        match &edit.state {
            TrackEditState::Dropped => Self {
                track_id: edit.track_id.clone(),
                dropped: true,
                file: None,
            },
            TrackEditState::Edited(track) => Self {
                track_id: edit.track_id.clone(),
                dropped: false,
                file: track.file.clone(),
            },
        }
    }
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

pub(crate) fn apply_track_mapping_edits(
    table: MappingTable,
    edits: &[CandidateTrackMappingEdit],
) -> MappingTable {
    edits.iter().fold(table, |table, edit| {
        if edit.dropped {
            return mapping_without_track(table, &edit.track_id);
        }
        let track = table
            .rows
            .iter()
            .flat_map(|row| row.units())
            .find_map(|unit| match &unit.becomes {
                crate::import::mapping::MappingBecomes::Track { track, .. }
                    if track.id == edit.track_id =>
                {
                    Some(track.clone())
                }
                _ => None,
            });
        match track {
            Some(mut track) => {
                track.file.clone_from(&edit.file);
                mapping_with_track(table, track)
            }
            None => table,
        }
    })
}
