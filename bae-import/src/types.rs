//! Import type definitions.
//!
//! Every import follows the same flow, whether the tracks are individual files
//! or one container plus a CUE sheet, and whether the metadata came from
//! MusicBrainz, Discogs, the files' own tags, or direct entry:
//!
//! 1. **Preparation** ([`PrepareStep`]) — validate that the stored candidate's
//!    source files still have the identities recorded by the scan.
//! 2. **Running** ([`ImportPhase`]) — read and hash each file where it already
//!    sits (no bytes move, no transcode), measure per-track loudness by decoding,
//!    and write every row in one transaction.
//!
//! An import always lands as a local, playable release. An
//! [`ImportDestination::Remote`] import then uploads to the cloud in the background.
use bae_catalogs::credits::{ArtistCredit, CreditedRole};
use bae_catalogs::works::WorkGraph;
use serde::{Deserialize, Serialize};
#[cfg(desktop)]
mod import_run;
#[cfg(desktop)]
pub use import_run::{
    CueAnalyzedAudioFile, CueFlacAnalysis, ImportCommand, ImportDestination, TrackAudio, TrackFile,
};
#[cfg(desktop)]
mod progress;
#[cfg(desktop)]
pub use progress::*;
mod candidate_edit_field;
pub use candidate_edit_field::{CandidateEditField, DraftFieldEdit, PressingFactEdit};
mod artist_assignment;
pub use artist_assignment::{
    artists_standing, credited_line, ArtistAssignment, ArtistStanding, ArtistsStanding, Credited,
};
mod raw_release_edit;
mod release_edit_display;
pub use raw_release_edit::{
    EditValidationError, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
};
pub use release_edit_display::{
    ReleaseEditDisplayContext, ReleaseEditLoadError, ReleaseEditTrackContext, ReleaseEditTrackRun,
    ReleaseEditTrackSide, ReleaseEditTrackSource,
};
mod candidate_draft;
pub use candidate_draft::{
    AudioFile, CandidateDraft, CandidateTrack, DraftTrack, LibraryDraft, LibraryTrack,
    MetadataProvenance, ReleaseDraft, TrackBinding,
};
#[cfg(desktop)]
pub use candidate_draft::{CandidateMetadataDraft, CandidatePreparedAssets};

/// What an import candidate is linked to in the catalogs: the pressing a
/// person picked, or identification picked unattended, for its folder; or,
/// where the person could not tell which of the pressings offered their copy
/// is, the album those pressings are of.
///
/// Independent of the draft. Only explicit actions set or clear it — a pick,
/// identification applying its pick, linking the album, keeping the
/// folder's own draft over the lookup, unlinking — so editing, clearing, or
/// re-reading the draft leaves it standing. Importing takes the draft's
/// values and this link's catalog identities.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseLink {
    Pressing(bae_identify::PressingLink),
    Album(bae_identify::AlbumLink),
}

#[cfg(desktop)]
impl ReleaseLink {
    /// The pressing the link names, where it names one.
    pub fn pressing(&self) -> Option<&bae_identify::PressingLink> {
        match self {
            Self::Pressing(pressing) => Some(pressing),
            Self::Album(_) => None,
        }
    }
}

/// Whether a track inherits its album artists or has its own ordered credits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackArtistAssignments {
    AlbumArtists,
    Explicit(Vec<Credited>),
}

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

/// Per-track user edits. Aligned positionally with the release's existing
/// tracks — element N edits track N (ordered as
/// `Database::get_tracks_for_release` returns them).
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
/// a library release's edit session's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftAlbum {
    /// A new album: none is picked and the draft's records name none the
    /// library holds.
    New,
    /// The album the draft's records name (see
    /// [`bae_db::Database::album_records_name`]); for a library release
    /// with no catalog applied, the album it is in.
    Named { album_id: String, title: String },
    /// The album the person picked, over what the records name.
    Picked { album_id: String, title: String },
}

/// A library release's edit session as the editor opens it: the raw form,
/// the cover, what the editor shows beside the form, and the album a save
/// files the release under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    pub cover: Option<bae_db::ImageRef>,
    pub display: crate::ReleaseEditDisplayContext,
    pub album: DraftAlbum,
}

impl ReleaseEditSeed {
    /// Whether this seed holds every value `opened` held — the form, the
    /// tracks and the album — whatever its cover: the cover is written at
    /// once outside any session, so it never makes a session's values stale.
    pub fn holds_the_values_of(&self, opened: &Self) -> bool {
        let Self { edit, cover: _, display, album } = self;
        *edit == opened.edit && *display == opened.display && *album == opened.album
    }
}

/// Trim a raw pressing field; empty (after trim) becomes `None`.
fn trim_to_option(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

impl TrackArtistAssignments {
    /// The track's own artists; none when it takes the album's.
    pub fn explicit(&self) -> &[Credited] {
        match self {
            Self::AlbumArtists => &[],
            Self::Explicit(assignments) => assignments,
        }
    }

    /// Each credit normalized and the joins kept as stored (see
    /// [`Credited::normalized_list`]).
    pub(crate) fn normalize(&mut self) {
        if let Self::Explicit(assignments) = self {
            *assignments = Credited::normalized_list(std::mem::take(assignments));
        }
    }
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
