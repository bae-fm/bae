//! The editor's form.
//!
//! The form is text as typed; [`RawReleaseEdit::shape`] turns it into the
//! wire edit and is the one place the typed text is validated. A candidate's
//! draft ([`super::CandidateDraft`]) projects its editable fields into this
//! form.

use super::*;
pub use bae_types::edit_validation::EditValidationError;

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
            && self.pressing.barcodes.iter().all(|barcode| barcode.trim().is_empty())
            && self.pressing.facts.is_empty()
            && self.tracks.iter().all(|track| {
                track.title.trim().is_empty()
                    && matches!(track.artist_assignments, TrackArtistAssignments::AlbumArtists)
            })
    }
}

impl RawReleaseEdit {
    /// Put one album-level field edit in the form: text typed into a field,
    /// the label or barcode rows as the person left them, or a choice of
    /// what the pressing is. A label row keeps the ids of the form's row with
    /// its name ([`RawLabelEdit::kept_ids`]), as a draft's does.
    pub fn set_field(&mut self, edit: DraftFieldEdit) {
        match edit {
            DraftFieldEdit::Text { field, value } => {
                let slot = match field {
                    CandidateEditField::AlbumTitle => &mut self.album_title,
                    CandidateEditField::AlbumYear => &mut self.album_year,
                    CandidateEditField::PressingYear => &mut self.pressing.year,
                };
                *slot = value;
            }
            DraftFieldEdit::Labels(labels) => {
                self.pressing.labels = RawLabelEdit::kept_ids(labels, &self.pressing.labels);
            }
            DraftFieldEdit::Barcodes(barcodes) => self.pressing.barcodes = barcodes,
            DraftFieldEdit::PressingFact(choice) => choice.apply(&mut self.pressing.facts),
        }
    }

    /// Replace the album's ordered credits, each normalized as a stored list
    /// is: names trimmed, each join kept only where it states something, the
    /// last credit's dropped.
    pub fn set_album_artists(&mut self, assignments: Vec<Credited>) {
        self.album_artist_assignments = Credited::normalized_list(assignments);
    }

    /// Put the title and artists of `edited` in the form's row with its id,
    /// its credits normalized; the row keeps its audio. `false` when the form
    /// has no row with that id.
    pub fn set_track(&mut self, edited: RawTrackEdit) -> bool {
        let Some(track) = self.tracks.iter_mut().find(|track| track.id == edited.id) else {
            return false;
        };
        let RawTrackEdit { title, mut artist_assignments, .. } = edited;
        artist_assignments.normalize();
        track.title = title;
        track.artist_assignments = artist_assignments;
        true
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
    pub facts: bae_types::pressing::PressingFacts,
}

/// One row of the form's label or barcode list as the editor draws it: one per
/// stored row, so a list with none draws no row. The first row carries the
/// list's heading and the last the control that adds another row; every row
/// can be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow<T> {
    pub row: T,
    /// Whether the row carries the list's heading.
    pub labelled: bool,
    /// Whether the row carries the control that adds another row.
    pub adds_row: bool,
}

impl<T: Clone> ListRow<T> {
    /// The rows the editor draws of the stored `rows`.
    fn list(rows: &[T]) -> Vec<Self> {
        let count = rows.len();
        rows.iter()
            .enumerate()
            .map(|(position, row)| Self {
                labelled: position == 0,
                adds_row: position + 1 == count,
                row: row.clone(),
            })
            .collect()
    }
}

impl ListRow<RawLabelEdit> {
    /// The rows the editor draws of the stored label rows `labels`.
    pub fn of_labels(labels: &[RawLabelEdit]) -> Vec<Self> {
        Self::list(labels)
    }
}

impl ListRow<String> {
    /// The rows the editor draws of the stored barcode rows `barcodes`.
    pub fn of_barcodes(barcodes: &[String]) -> Vec<Self> {
        Self::list(barcodes)
    }
}

impl RawPressingEdit {
    /// The label rows the editor draws.
    pub fn label_rows(&self) -> Vec<ListRow<RawLabelEdit>> {
        ListRow::of_labels(&self.labels)
    }

    /// The barcode rows the editor draws.
    pub fn barcode_rows(&self) -> Vec<ListRow<String>> {
        ListRow::of_barcodes(&self.barcodes)
    }
}

/// One label row of the form, each field as typed, empty meaning "not set",
/// with the ids the catalog that stated the row knows its label by. The ids
/// belong to the name they came with: the editors show and send the name and
/// number alone, and [`RawLabelEdit::kept_ids`] gives a row back the ids of
/// the row it was, while it keeps that row's name.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLabelEdit {
    pub name: String,
    pub catalog_number: String,
    pub musicbrainz_label_id: Option<String>,
    pub discogs_label_id: Option<String>,
}

impl RawLabelEdit {
    /// Whether the row states nothing.
    pub fn is_blank(&self) -> bool {
        self.name.trim().is_empty() && self.catalog_number.trim().is_empty()
    }

    /// `rows` as an editor left them, each with the ids of the row of
    /// `stated` that has its name: the row at its own place where that one
    /// has it, else the first. A row whose name no stated row has, typed or
    /// changed, carries no ids, so a label the person names is found by its
    /// name.
    pub fn kept_ids(rows: Vec<Self>, stated: &[Self]) -> Vec<Self> {
        rows.into_iter()
            .enumerate()
            .map(|(position, row)| {
                let same_name = |stated: &&Self| stated.name == row.name;
                let source = stated
                    .get(position)
                    .filter(same_name)
                    .or_else(|| stated.iter().find(same_name));
                Self {
                    musicbrainz_label_id: source
                        .and_then(|source| source.musicbrainz_label_id.clone()),
                    discogs_label_id: source.and_then(|source| source.discogs_label_id.clone()),
                    ..row
                }
            })
            .collect()
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

/// Typed text trimmed; empty after trimming is `None`.
pub(super) fn trim_to_option(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[expect(
    clippy::map_err_ignore,
    reason = "the year is the person's own text; which character failed adds nothing to it"
)]
fn parse_optional_year(raw: &str) -> Result<Option<i32>, EditValidationError> {
    match raw.trim() {
        "" => Ok(None),
        text => text.parse::<i32>().map(Some).map_err(|_| EditValidationError::InvalidYear),
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
            album_year: edit.album_year.map(|year| year.to_string()).unwrap_or_default(),
            pressing: RawPressingEdit::from_pressing(&edit.pressing),
            tracks,
        }
    }
}

impl RawTrackEdit {
    /// Seed one raw editor row from a wire [`TrackUserEdit`], under the row
    /// identity `id`, retaining artist assignments and the audio binding.
    pub fn from_user_edit(edit: TrackUserEdit, id: String) -> Self {
        Self { id, title: edit.title, artist_assignments: edit.artist_assignments, file: edit.file }
    }
}

/// What a summary of a draft states of its release, as the parts of its
/// edition and copy lines.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftReleaseLines {
    /// The year the pressing states, as typed; `None` when it states none.
    pub year: Option<String>,
    /// The labels as a surface shows them: each name once, each catalog
    /// number once.
    pub labels: Vec<bae_types::pressing::LabelLine>,
    /// Where the pressing was released, its media, status, packaging and
    /// Discogs details.
    pub facts: bae_types::pressing::PressingFacts,
    /// How long the draft's tracks play together; `None` when it has none.
    pub length: Option<bae_util::duration::DurationUnits>,
}

/// What the pressing's Add menus offer: each medium and Discogs detail the
/// pressing's facts do not hold yet, in the vocabulary's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressingOffers {
    pub media: Vec<bae_types::pressing::Medium>,
    pub discogs_details: Vec<bae_types::pressing::DiscogsDetail>,
}

impl RawPressingEdit {
    /// What a summary of the draft states of its release: the parts of the
    /// edition and copy lines, with `length_ms`, the length the draft's
    /// tracks measure together.
    pub fn release_lines(&self, length_ms: u64) -> DraftReleaseLines {
        DraftReleaseLines {
            year: trim_to_option(&self.year),
            labels: bae_types::pressing::label_lines(&self.stated_labels()),
            facts: self.facts.clone(),
            length: bae_util::duration::DurationUnits::from_millis(length_ms),
        }
    }

    /// The labels the rows state, each once, with the ids the catalogs know
    /// them by; a blank row states none.
    fn stated_labels(&self) -> Vec<bae_types::pressing::ReleaseLabel> {
        bae_types::pressing::ReleaseLabel::each_once(self.labels.iter().filter_map(|label| {
            bae_types::pressing::ReleaseLabel::new(
                trim_to_option(&label.name),
                trim_to_option(&label.catalog_number),
            )
            .map(|stated| {
                stated.with_ids(label.musicbrainz_label_id.clone(), label.discogs_label_id.clone())
            })
        }))
    }

    /// The choices the pressing's Add menus offer.
    pub fn offers(&self) -> PressingOffers {
        PressingOffers {
            media: bae_types::pressing::Medium::ALL
                .iter()
                .copied()
                .filter(|medium| !self.facts.media.iter().any(|held| held.medium == *medium))
                .collect(),
            discogs_details: bae_types::pressing::DiscogsDetail::ALL
                .iter()
                .copied()
                .filter(|detail| !self.facts.discogs_details.contains(detail))
                .collect(),
        }
    }

    /// Parse and normalize raw pressing text into a wire
    /// [`Pressing`](bae_types::pressing::Pressing): empty fields become `None`,
    /// blank and repeated label and barcode rows are dropped, and the year
    /// parses.
    fn shape(&self) -> Result<bae_types::pressing::Pressing, EditValidationError> {
        Ok(bae_types::pressing::Pressing {
            year: parse_optional_year(&self.year)?,
            labels: self.stated_labels(),
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

    /// Render a wire [`Pressing`](bae_types::pressing::Pressing) back to raw
    /// editor text.
    pub fn from_pressing(pressing: &bae_types::pressing::Pressing) -> Self {
        Self {
            year: pressing.year.map(|y| y.to_string()).unwrap_or_default(),
            labels: pressing
                .labels
                .iter()
                .map(|label| RawLabelEdit {
                    name: label.name().unwrap_or_default().to_string(),
                    catalog_number: label.catalog_number().unwrap_or_default().to_string(),
                    musicbrainz_label_id: label.musicbrainz_label_id().map(str::to_string),
                    discogs_label_id: label.discogs_label_id().map(str::to_string),
                })
                .collect(),
            barcodes: pressing.barcodes.clone(),
            facts: pressing.facts.clone(),
        }
    }
}

#[cfg(test)]
#[path = "raw_release_edit_tests.rs"]
mod tests;
