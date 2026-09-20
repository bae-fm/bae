//! The editor's form and the stored draft that shares its shape.
//!
//! The form is text as typed; [`RawReleaseEdit::shape`] turns it into the
//! wire edit and is the one place the typed text is validated. A candidate's
//! draft is the same album fields over rows that also carry each track's
//! physical decision.

use super::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportError;

/// Edit-metadata form values exactly as the editor holds them — text the user
/// typed, not yet normalized. Artist assignments retain their identity while
/// pressing fields are raw strings (empty means "not set"); `year` is text.
///
/// The editor binds directly to this shape and calls
/// [`shape`](RawReleaseEdit::shape) both to gate its Save button and to build
/// the commit payload: it trims, splits, parses, and validates into a wire
/// [`ReleaseUserEdit`]. [`from_user_edit`](RawReleaseEdit::from_user_edit) is
/// the reverse, seeding this form from a wire edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawReleaseEditOf<Track> {
    pub album_title: String,
    pub album_artist_assignments: Vec<ArtistAssignment>,
    pub album_year: String,
    pub pressing: RawPressingEdit,
    pub tracks: Vec<Track>,
    /// Where each album-level value above came from. It travels with the
    /// values rather than beside them, so every projection, edit and store
    /// that carries one carries the other.
    pub origins: FieldOrigins,
}

/// The editor's form: album fields over the rows a person edits.
pub type RawReleaseEdit = RawReleaseEditOf<RawTrackEdit>;

/// A candidate's stored draft: the same album fields over one
/// [`CandidateTrack`] per audio slot, each carrying its physical decision
/// beside its metadata. One row per slot is what makes "every track has one
/// file binding" true by construction rather than by a check.
pub type CandidateDraft = RawReleaseEditOf<CandidateTrack>;

/// A track included in the candidate. Its audio and number are required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateTrack {
    pub edit: RawTrackEdit<AudioFile, i32>,
    /// The track in the applied source document that supplied supplemental
    /// credits. Audio replacements have no corresponding source track.
    pub source_index: Option<u32>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl CandidateTrack {
    pub fn from_edit(edit: RawTrackEdit, position: usize) -> Result<Self, ImportError> {
        let file = edit.file.ok_or_else(|| ImportError::Internal {
            detail: format!("draft track {} has no audio", edit.id),
        })?;
        Ok(Self {
            edit: RawTrackEdit {
                id: edit.id,
                title: edit.title,
                artist_assignments: edit.artist_assignments,
                side: edit.side,
                track_number: edit.track_number.unwrap_or_else(|| {
                    i32::try_from(position + 1).expect("track position fits i32")
                }),
                file,
            },
            source_index: None,
        })
    }
}

impl RawTrackEdit<AudioFile, i32> {
    /// Cross the editor boundary without weakening the stored draft's shape.
    pub fn as_edit(&self) -> RawTrackEdit {
        RawTrackEdit {
            id: self.id.clone(),
            title: self.title.clone(),
            artist_assignments: self.artist_assignments.clone(),
            side: self.side,
            track_number: Some(self.track_number),
            file: Some(self.file.clone()),
        }
    }
}

impl CandidateDraft {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn audio_durations(
        &self,
        durations: &crate::import::probe::SourceDurations,
    ) -> Result<Vec<u64>, ImportError> {
        self.tracks
            .iter()
            .map(|track| {
                durations
                    .duration_of(&track.edit.file)
                    .ok_or_else(|| ImportError::Internal {
                        detail: format!("draft track {} has no measured audio", track.edit.id),
                    })
            })
            .collect()
    }

    pub fn release_edit(&self) -> RawReleaseEdit {
        RawReleaseEditOf {
            album_title: self.album_title.clone(),
            album_artist_assignments: self.album_artist_assignments.clone(),
            album_year: self.album_year.clone(),
            pressing: self.pressing.clone(),
            tracks: self
                .tracks
                .iter()
                .map(|track| track.edit.as_edit())
                .collect(),
            origins: self.origins.clone(),
        }
    }
}

impl RawReleaseEdit {
    /// Whether the editable metadata contains no authored or sourced values.
    /// Physical side/track positions are excluded: they describe candidate
    /// slots, not release metadata.
    pub fn is_blank(&self) -> bool {
        self.album_title.trim().is_empty()
            && self.album_artist_assignments.is_empty()
            && self.album_year.trim().is_empty()
            && self.pressing.year.trim().is_empty()
            && self.pressing.format.trim().is_empty()
            && self.pressing.label.trim().is_empty()
            && self.pressing.catalog_number.trim().is_empty()
            && self.pressing.country.trim().is_empty()
            && self.pressing.barcode.trim().is_empty()
            && self.tracks.iter().all(|track| {
                track.title.trim().is_empty()
                    && matches!(
                        track.artist_assignments,
                        TrackArtistAssignments::AlbumArtists
                    )
            })
    }

    /// Discogs identities on new album artists and on track artists whose rows
    /// have audio bound. Existing-library assignments need no prepared image
    /// because import does not insert those artists; fileless rows do not
    /// become tracks.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn new_discogs_artist_ids_for_bound_tracks(
        &self,
    ) -> std::collections::BTreeSet<String> {
        self.album_artist_assignments
            .iter()
            .chain(self.tracks.iter().flat_map(|track| {
                match (&track.artist_assignments, track.file.is_some()) {
                    (TrackArtistAssignments::Explicit(assignments), true) => assignments.as_slice(),
                    _ => &[],
                }
            }))
            .filter_map(|assignment| match assignment {
                ArtistAssignment::Existing { .. } => None,
                ArtistAssignment::New { seed } => seed.discogs_artist_id.clone(),
            })
            .collect()
    }
}

/// Raw pressing fields as the editor holds them: each is the text the user
/// typed, empty meaning "not set". `year` is text because the form is
/// text; `shape` parses it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawPressingEdit {
    pub year: String,
    pub format: String,
    pub label: String,
    pub catalog_number: String,
    pub country: String,
    pub barcode: String,
}

/// One raw track row from the editor. `id` is the editor's stable row
/// identity (existing track id post-commit, or a synthetic
/// `{prefix}-{index}` from [`from_user_edit`](RawReleaseEdit::from_user_edit))
/// — used only to diff rows in the UI; shaping drops it because wire
/// tracks zip positionally to existing track IDs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawTrackEdit<Audio = Option<AudioFile>, Number = Option<i32>> {
    pub id: String,
    pub title: String,
    pub artist_assignments: TrackArtistAssignments,
    pub side: Option<i32>,
    pub track_number: Number,
    /// The audio bound to this row, carried through editing untouched. This is
    /// what makes a pairing correctable: `shape` keeps it, so what the user
    /// left in the slot table is what the commit writes.
    pub file: Audio,
}

/// Why a [`RawReleaseEdit`] can't be shaped into a savable
/// [`ReleaseUserEdit`]. The editor surfaces this to disable Save (and, on
/// commit, to block the write).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditValidationError {
    #[error("Album title is required")]
    EmptyAlbumTitle,
    #[error("Album must have at least one artist")]
    NoAlbumArtist,
    #[error("Artist name is required")]
    EmptyArtistName,
    #[error("Year must be a number")]
    InvalidYear,
}

/// Render an optional pressing field back to raw editor text: `None`
/// becomes empty, `Some(v)` becomes `v`.
fn option_to_raw(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn parse_optional_year(raw: &str) -> Result<Option<i32>, EditValidationError> {
    match raw.trim() {
        "" => Ok(None),
        text => text
            .parse::<i32>()
            .map(Some)
            .map_err(|_| EditValidationError::InvalidYear),
    }
}

impl RawReleaseEdit {
    /// Normalize and validate this raw form into a wire [`ReleaseUserEdit`]:
    /// trim the album title and new-artist names, parse the year, and map empty
    /// pressing fields to `None`. `Ok` means the
    /// form is savable.
    ///
    /// Validation: the album title must be non-empty after trimming, the album
    /// must resolve to at least one artist (both via
    /// [`ReleaseUserEdit::validate`]), and a non-empty year must parse as an
    /// integer (an empty year is allowed — it's "not set").
    pub fn shape(&self) -> Result<ReleaseUserEdit, EditValidationError> {
        let edit = ReleaseUserEdit {
            album_title: self.album_title.clone(),
            album_artist_assignments: self.album_artist_assignments.clone(),
            album_year: parse_optional_year(&self.album_year)?,
            pressing: self.pressing.shape()?,
            tracks: self
                .tracks
                .iter()
                .map(|t| TrackUserEdit {
                    title: t.title.clone(),
                    side: t.side,
                    track_number: t.track_number,
                    artist_assignments: t.artist_assignments.clone(),
                    file: t.file.clone(),
                })
                .collect(),
            origins: self.origins.clone(),
        }
        .normalized();
        edit.validate()?;
        Ok(edit)
    }

    /// Seed a raw editor form from a wire [`ReleaseUserEdit`]. Retains artist
    /// assignments and renders absent pressing fields as empty strings.
    /// `track_id_prefix` provides the
    /// editor row identities the wire edit lacks: track N becomes
    /// `{track_id_prefix}-{N}`.
    pub fn from_user_edit(edit: ReleaseUserEdit, track_id_prefix: &str) -> Self {
        let tracks = edit
            .tracks
            .into_iter()
            .enumerate()
            .map(|(index, t)| RawTrackEdit::from_user_edit(t, format!("{track_id_prefix}-{index}")))
            .collect();

        Self {
            album_title: edit.album_title,
            album_artist_assignments: edit.album_artist_assignments,
            album_year: edit
                .album_year
                .map(|year| year.to_string())
                .unwrap_or_default(),
            pressing: RawPressingEdit::from_pressing(&edit.pressing),
            tracks,
            origins: edit.origins,
        }
    }
}

impl<Track> RawReleaseEditOf<Track> {
    /// The same form with every field it states read from `origin`, and
    /// nothing said about the fields it leaves blank — what a projection of
    /// one source produces.
    pub fn read_from(mut self, origin: FieldOrigin) -> Self {
        self.origins = FieldOrigins::of(&FieldValues::of_draft(&self), origin);
        self
    }
}

impl RawTrackEdit {
    /// Seed one raw editor row from a wire [`TrackUserEdit`], under the row
    /// identity `id`, retaining artist assignments and the audio binding.
    pub fn from_user_edit(edit: TrackUserEdit, id: String) -> Self {
        Self {
            id,
            title: edit.title,
            artist_assignments: edit.artist_assignments,
            side: edit.side,
            track_number: edit.track_number,
            file: edit.file,
        }
    }
}

impl RawPressingEdit {
    /// Parse and normalize raw pressing text into a wire [`PressingEdit`]:
    /// empty fields become `None`; the year parses as an integer.
    fn shape(&self) -> Result<PressingEdit, EditValidationError> {
        Ok(PressingEdit {
            year: parse_optional_year(&self.year)?,
            format: trim_to_option(&self.format),
            label: trim_to_option(&self.label),
            catalog_number: trim_to_option(&self.catalog_number),
            country: trim_to_option(&self.country),
            barcode: trim_to_option(&self.barcode),
        })
    }

    /// Render a wire [`PressingEdit`] back to raw editor text.
    pub fn from_pressing(pressing: &PressingEdit) -> Self {
        Self {
            year: pressing.year.map(|y| y.to_string()).unwrap_or_default(),
            format: option_to_raw(&pressing.format),
            label: option_to_raw(&pressing.label),
            catalog_number: option_to_raw(&pressing.catalog_number),
            country: option_to_raw(&pressing.country),
            barcode: option_to_raw(&pressing.barcode),
        }
    }
}
