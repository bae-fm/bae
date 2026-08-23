//! What a person typed over a candidate's metadata, and what became of an
//! import that failed.
//!
//! Every one of these is written the moment it happens and read back by the
//! per-candidate query, so the pane holds no copy of its own: the control
//! writes, the query redraws.

use crate::import::mapping::{mapping_with_track, mapping_without_track, MappingTable};
use crate::import::types::{AudioFile, RawReleaseEdit, RawTrackEdit};
use chrono::{DateTime, Utc};

/// One album-level field of the metadata form.
///
/// The form's own fields, not the wire edit's: `year` is text here because the
/// field is text, and the commit is what parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEditField {
    AlbumTitle,
    AlbumArtistText,
    Year,
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
            Self::AlbumArtistText => "album_artist_text",
            Self::Year => "year",
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
    pub album_artist_text: Option<String>,
    pub year: Option<String>,
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
        overwrite(&mut seed.album_artist_text, &self.album_artist_text);
        overwrite(&mut seed.pressing.year, &self.year);
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
/// after a relaunch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub error: String,
    pub failed_at: DateTime<Utc>,
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
