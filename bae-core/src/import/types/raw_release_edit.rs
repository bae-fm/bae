//! The editor's form.
//!
//! The form is text as typed; [`RawReleaseEdit::shape`] turns it into the
//! wire edit and is the one place the typed text is validated. A candidate's
//! draft ([`super::CandidateDraft`]) projects its editable fields into this
//! form.

use super::*;

/// Edit-metadata form values exactly as the editor holds them — text the user
/// typed, not yet normalized. Artist assignments retain their identity while
/// pressing text fields are raw strings (empty means "not set"); `year` is
/// text. What the pressing is — its area, media, status, packaging and
/// details — is chosen rather than typed, so the form holds it typed.
///
/// The editor binds directly to this shape and calls
/// [`shape`](RawReleaseEdit::shape) both to gate its Save button and to build
/// the commit payload: it trims, splits, parses, and validates into a wire
/// [`ReleaseUserEdit`]. [`from_user_edit`](RawReleaseEdit::from_user_edit) is
/// the reverse, seeding this form from a wire edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawReleaseEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<Credited>,
    pub album_year: String,
    pub pressing: RawPressingEdit,
    pub tracks: Vec<RawTrackEdit>,
}

impl RawReleaseEdit {
    /// Whether the editable metadata contains no authored or sourced values.
    pub fn is_blank(&self) -> bool {
        self.album_title.trim().is_empty()
            && self.album_artist_assignments.is_empty()
            && self.album_year.trim().is_empty()
            && self.pressing.year.trim().is_empty()
            && self.pressing.labels.iter().all(RawLabelEdit::is_blank)
            && self
                .pressing
                .barcodes
                .iter()
                .all(|barcode| barcode.trim().is_empty())
            && self.pressing.facts.is_empty()
            && self.tracks.iter().all(|track| {
                track.title.trim().is_empty()
                    && matches!(
                        track.artist_assignments,
                        TrackArtistAssignments::AlbumArtists
                    )
            })
    }
}

impl RawReleaseEdit {
    /// Every artist credit the form carries, the album's first, then each
    /// track's own; picked library artists are not credits.
    pub(crate) fn credits(&self) -> impl Iterator<Item = &ArtistCredit> {
        self.album_artist_assignments
            .iter()
            .chain(
                self.tracks
                    .iter()
                    .flat_map(|track| track.artist_assignments.explicit()),
            )
            .filter_map(|credited| credited.artist.credit())
    }
}

/// Raw pressing fields as the editor holds them: each text field is the text
/// the user typed, empty meaning "not set". `year` is text because the form
/// is text; `shape` parses it. `facts` is what the person chose from bae's
/// vocabulary, and needs no parsing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawPressingEdit {
    pub year: String,
    /// One row per label; shaping leaves out a blank row.
    pub labels: Vec<RawLabelEdit>,
    /// One row per barcode, as typed; shaping leaves out a blank row and a
    /// repeated one.
    pub barcodes: Vec<String>,
    pub facts: crate::pressing::PressingFacts,
}

/// One label row of the form, each field as typed, empty meaning "not set".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLabelEdit {
    pub name: String,
    pub catalog_number: String,
}

impl RawLabelEdit {
    /// Whether the row states nothing.
    pub fn is_blank(&self) -> bool {
        self.name.trim().is_empty() && self.catalog_number.trim().is_empty()
    }
}

/// One raw track row from the editor. `id` is the editor's stable row
/// identity (existing track id post-commit, or a synthetic
/// `{prefix}-{index}` from [`from_user_edit`](RawReleaseEdit::from_user_edit))
/// — used only to diff rows in the UI; shaping drops it because wire
/// tracks zip positionally to existing track IDs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawTrackEdit<Audio = Option<AudioFile>> {
    pub id: String,
    pub title: String,
    pub artist_assignments: TrackArtistAssignments,
    /// The audio bound to this row, carried through editing untouched, so the
    /// row the editor hands back names the audio it was drawn over.
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

#[expect(
    clippy::map_err_ignore,
    reason = "the year is the person's own text; which character failed adds nothing to it"
)]
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
                    artist_assignments: t.artist_assignments.clone(),
                    file: t.file.clone(),
                })
                .collect(),
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
        }
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
            file: edit.file,
        }
    }
}

impl RawPressingEdit {
    /// Parse and normalize raw pressing text into a wire
    /// [`Pressing`](crate::pressing::Pressing): empty fields become `None`,
    /// blank and repeated label and barcode rows are dropped, and the year
    /// parses.
    fn shape(&self) -> Result<crate::pressing::Pressing, EditValidationError> {
        Ok(crate::pressing::Pressing {
            year: parse_optional_year(&self.year)?,
            labels: crate::pressing::ReleaseLabel::list(self.labels.iter().map(|label| {
                (
                    trim_to_option(&label.name),
                    trim_to_option(&label.catalog_number),
                )
            })),
            barcodes: {
                let mut barcodes: Vec<String> = Vec::new();
                for barcode in self.barcodes.iter().filter_map(|row| trim_to_option(row)) {
                    if !barcodes.contains(&barcode) {
                        barcodes.push(barcode);
                    }
                }
                barcodes
            },
            facts: self.facts.clone(),
        })
    }

    /// Render a wire [`Pressing`](crate::pressing::Pressing) back to raw
    /// editor text.
    pub fn from_pressing(pressing: &crate::pressing::Pressing) -> Self {
        Self {
            year: pressing.year.map(|y| y.to_string()).unwrap_or_default(),
            labels: pressing
                .labels
                .iter()
                .map(|label| RawLabelEdit {
                    name: label.name().unwrap_or_default().to_string(),
                    catalog_number: label.catalog_number().unwrap_or_default().to_string(),
                })
                .collect(),
            barcodes: pressing.barcodes.clone(),
            facts: pressing.facts.clone(),
        }
    }
}
