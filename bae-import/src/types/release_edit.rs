//! A release edit as it is written: the values an edit saves and the rule
//! every saved edit holds, and the form a library release's editor holds.

use super::*;

/// Every field the edit-metadata sheet may change. Artist choices preserve
/// whether the person picked a library artist or left a credit; a credit is
/// resolved against the library when the edit commits.
///
/// Records and provenance are out of scope: the release's records and the
/// release's metadata provenance are untouched. So is the stored catalog
/// release, so a later reset can still re-seed from what the source said.
///
/// For a release already in the library, `tracks` MUST have the same length as
/// the release's existing tracks; that editor cannot add or remove tracks
/// (that's a re-import, not an edit). An import's tracks are the included audio
/// units, and metadata application must describe that same ordered list.
///
/// `album_artist_assignments` is the album's credit list in order; element *n*
/// is the `album_artists` row at position *n*.
/// Empty is a validation error: every album has at least one artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<Credited>,
    pub album_year: Option<i32>,
    /// The pressing the edit claims: either the whole of a picked release's,
    /// or [`bae_types::pressing::Pressing::blank()`] filled in with what the person knows (file
    /// metadata or direct entry).
    pub pressing: bae_types::pressing::Pressing,
    pub tracks: Vec<TrackUserEdit>,
}

impl ReleaseUserEdit {
    /// Trim the album title and every track title, and drop blank artist names.
    /// The normalization the editor's [`RawReleaseEdit::shape`] performs on typed
    /// text, hoisted onto the wire type so MCP, which builds one
    /// field-for-field, gets the same treatment instead of
    /// writing whatever it was handed. Idempotent.
    pub fn normalized(mut self) -> Self {
        self.album_title = self.album_title.trim().to_string();
        self.album_artist_assignments = Credited::normalized_list(self.album_artist_assignments);
        for track in &mut self.tracks {
            track.title = track.title.trim().to_string();
            track.artist_assignments.normalize();
        }
        self
    }

    /// The invariants a user-submitted edit holds: a non-blank album title, and
    /// at least one album artist. The single definition of the rule — the editor
    /// gates its Save button on it through [`RawReleaseEdit::shape`], and the
    /// library's write of a person's release edit enforces it on the write
    /// itself, so a surface that never opens the editor cannot write past it.
    ///
    /// Only a *user edit* is held to this. A release reseeded from sparse file
    /// tags carries deliberate blanks for the user to fill, and takes the write
    /// path that doesn't gate.
    pub fn validate(&self) -> Result<(), EditValidationError> {
        if self.album_title.trim().is_empty() {
            return Err(EditValidationError::EmptyAlbumTitle);
        }
        if self.album_artist_assignments.is_empty() {
            return Err(EditValidationError::NoAlbumArtist);
        }
        for assignment in
            self.album_artist_assignments.iter().chain(self.tracks.iter().flat_map(|track| {
                match &track.artist_assignments {
                    TrackArtistAssignments::AlbumArtists => [].as_slice(),
                    TrackArtistAssignments::Explicit(assignments) => assignments.as_slice(),
                }
            }))
        {
            if assignment.artist.is_blank() {
                return Err(EditValidationError::EmptyArtistName);
            }
        }
        Ok(())
    }
}

/// Per-track user edits. Aligned positionally with the release's existing
/// tracks — element N edits track N (ordered as
/// `Records::get_tracks_for_release` returns them).
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackUserEdit {
    pub title: String,
    pub artist_assignments: TrackArtistAssignments,
    /// The audio chosen for an import track. Import drafts require it.
    /// The library metadata editor uses `None` because it retains the stored
    /// audio without changing it.
    pub file: Option<AudioFile>,
}

/// The library album a draft files under when it is written: an import's, or
/// a library release editor's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DraftAlbum {
    /// A new album, because the person picked no album and the draft's
    /// records name none the library holds.
    New,
    /// A new album the person picked, over what the records name.
    PickedNew,
    /// The album the draft's records name (see
    /// [`bae_records::Records::album_records_name`]); for a library release
    /// with no catalog applied, the album it is in.
    Named { album_id: String, line: AlbumLine },
    /// The album the person picked, over what the records name.
    Picked { album_id: String, line: AlbumLine },
}

/// How the album pick names a library album: its title, then its credit
/// line when the album credits anyone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlbumLine {
    /// The album credits no one, so its title names it alone.
    TitleAlone {
        title: String,
    },
    TitleAndArtists {
        title: String,
        artist_line: bae_types::credits::CreditLine,
    },
}

impl AlbumLine {
    /// The line of an album titled `title` whose credits print as
    /// `artist_line`.
    pub fn of(title: String, artist_line: bae_types::credits::CreditLine) -> Self {
        if artist_line.is_empty() {
            Self::TitleAlone { title }
        } else {
            Self::TitleAndArtists { title, artist_line }
        }
    }
}

/// What the person chose for the album a draft files under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlbumPick {
    /// Nothing: the album the draft's records name, else (for a library
    /// release) the album it is in, else a new album.
    Records,
    /// A new album, even where the records name one the library holds.
    New,
    /// This library album, over what the records name.
    Album { album_id: String },
}

/// A library release's values as an editor opens on them: the raw form,
/// what the editor shows beside the form, and the album the release is in.
/// The cover is not among them: the cover picker writes it at once, outside
/// any form, so it never makes an editor's values stale. A save refuses a
/// release whose values are no longer these.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    pub display: crate::ReleaseEditDisplayContext,
    pub album: DraftAlbum,
}

/// What applying a catalog release in a release editor read: every value,
/// which the apply lays onto the release's tracks in order, and the records
/// the pick names, the applied one first, which link the release to it.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCatalog {
    pub draft: crate::LibraryDraft,
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
}

/// The form a library release's editor holds, and every read and save of
/// the editor takes: what the editor opened on, the form with each change
/// the person made, the values the person applied, the records a save
/// links the release to and the album they picked. Each change makes a new
/// form; nothing in core keeps one.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditForm {
    pub release_id: String,
    /// The release as the library held it when the editor opened, or when
    /// its last save wrote it. A save refuses a release the library no
    /// longer holds so.
    pub opened: ReleaseEditSeed,
    pub edit: RawReleaseEdit,
    /// Every value the person applied over the release's own: a catalog
    /// release's, the files' tags, or the cleared values. A save writes each
    /// one, so nothing the release held before survives. `None` while the
    /// form holds the release's own values.
    pub applied: Option<crate::LibraryDraft>,
    /// The records a save links the release to in place of its own: an
    /// applied catalog's, the applied one first, or none once the person
    /// unlinked the release. `None` keeps the release's own.
    pub records: Option<Vec<bae_catalogs::records::ReleaseRecord>>,
    pub album_pick: AlbumPick,
    /// The side or disc the person picked for each CUE sheet, by the
    /// sheet's file id, that moves the sheet's tracks there. A catalog
    /// applied after a pick lays the tracks out at its own places and clears
    /// the picks.
    pub sheet_places: std::collections::BTreeMap<String, bae_types::mediums::MediumSide>,
    /// The audio file the person picked for each `FILE` line of a CUE
    /// sheet, by the sheet's file id and the line's text: a save binds the
    /// line to that file, reading it first. A catalog applied after a pick
    /// keeps it.
    pub sheet_files: Vec<bae_records::records::sheet_files::SheetFilePick>,
}

#[cfg(desktop)]
impl ReleaseEditForm {
    /// The form an editor opens with on `release_id` as `seed` holds it:
    /// nothing edited, applied or picked.
    pub fn opened_on(release_id: String, seed: ReleaseEditSeed) -> Self {
        Self {
            release_id,
            edit: seed.edit.clone(),
            opened: seed,
            applied: None,
            records: None,
            album_pick: AlbumPick::Records,
            sheet_places: std::collections::BTreeMap::new(),
            sheet_files: Vec::new(),
        }
    }

    /// Whether a save would write anything the editor did not open on.
    pub fn changed(&self) -> bool {
        self.edit != self.opened.edit
            || self.applied.is_some()
            || self.records.is_some()
            || self.album_pick != AlbumPick::Records
            || !self.sheet_places.is_empty()
            || !self.sheet_files.is_empty()
    }

    /// The form with `applied`'s values in place of the form's and of the
    /// values applied before, linked to its records, and no side picked for
    /// any CUE sheet, since the catalog places the tracks; the album the
    /// person picked stays picked.
    pub fn applying(self, applied: AppliedCatalog) -> Self {
        Self {
            edit: applied.draft.release_edit(),
            applied: Some(applied.draft),
            records: Some(applied.records),
            sheet_places: std::collections::BTreeMap::new(),
            ..self
        }
    }

    /// The form with `values` in place of the form's and of the values
    /// applied before: the files' tags, or the cleared values. The records,
    /// the sides picked for CUE sheets and the album pick stay as they are.
    pub fn applying_values(self, values: crate::LibraryDraft) -> Self {
        Self { edit: values.release_edit(), applied: Some(values), ..self }
    }

    /// The form linked to no record: a save drops the release's records,
    /// and every value stays as the form holds it.
    pub fn unlinked(self) -> Self {
        Self { records: Some(Vec::new()), ..self }
    }

    /// The form with the CUE sheet `sheet_file_id` picked onto `side`; a
    /// side the release lacks moves nothing. A pick of the side the editor
    /// opened the sheet on, with no catalog applied, is no pick.
    pub fn with_sheet_side(
        mut self,
        sheet_file_id: String,
        side: bae_types::mediums::MediumSide,
    ) -> Self {
        let opened = self.applied.is_none()
            && self.opened.display.tracks.iter().any(|track| {
                track
                    .sheet
                    .as_ref()
                    .is_some_and(|sheet| sheet.file_id == sheet_file_id && sheet.place.side == side)
            });
        if opened {
            self.sheet_places.remove(&sheet_file_id);
        } else {
            self.sheet_places.insert(sheet_file_id, side);
        }
        self
    }

    /// The form with the `FILE` line `file_reference` of the CUE sheet
    /// `sheet_file_id` picked onto the release's audio file
    /// `audio_file_id`. A pick of the file the editor opened the line on is
    /// no pick.
    pub fn with_sheet_file(
        mut self,
        sheet_file_id: String,
        file_reference: String,
        audio_file_id: String,
    ) -> Self {
        let opened = self.opened.display.tracks.iter().any(|track| {
            track.source.file_id == audio_file_id
                && track.sheet.as_ref().is_some_and(|sheet| {
                    sheet.file_id == sheet_file_id && sheet.file_reference == file_reference
                })
        });
        self.sheet_files.retain(|pick| {
            pick.sheet_file_id != sheet_file_id || pick.file_reference != file_reference
        });
        if !opened {
            self.sheet_files.push(bae_records::records::sheet_files::SheetFilePick {
                sheet_file_id,
                file_reference,
                audio_file_id,
            });
        }
        self
    }
}

/// A library release's editor's draft, as the editor shows it beside one
/// form, which the editor draws itself: the release's cover as it is now,
/// the tracks beside their form rows, the album a save files the release
/// under, how the form's album artists stand to the library, whether a save
/// would write anything the editor did not open on, the files it reads
/// first, and why the form can't be saved, when it can't.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditDraft {
    pub cover: Option<bae_records::ImageRef>,
    /// The summary of the release's audio.
    pub source_audio: Option<bae_records::source_audio::SourceAudioSummary>,
    /// The release's tracks on the track table's sides and runs, each row
    /// the form row that edits the track, with its picked artists as
    /// the library holds them now and each one the library no longer holds
    /// as a credit of the values the form holds for it; `None` when the
    /// release has no tracks.
    pub sides: Option<Vec<crate::TrackTableSide>>,
    /// The files the release carries that no track plays from, in the
    /// release's file order; `None` when it carries none.
    pub files: Option<Vec<crate::MappingFile>>,
    pub album: DraftAlbum,
    /// The catalogs that describe the release the form holds: the applied
    /// catalog's records, the applied one first, else the release's own.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// Whether `records` holds any.
    pub has_records: bool,
    /// The media and Discogs details the form's pressing Add menus offer.
    pub pressing_offers: crate::PressingOffers,
    /// The form's album artists, each with how it stands to the library, as
    /// this read found them; `None` when the form names no album artist.
    pub album_artists: Option<crate::ArtistField>,
    pub changed: bool,
    /// The names of the audio files a save reads before it writes, the ones
    /// the form binds CUE sheets' `FILE` lines to, in name order; `None`
    /// when a save reads none.
    pub files_to_read: Option<Vec<String>>,
    pub invalid: Option<EditValidationError>,
}
