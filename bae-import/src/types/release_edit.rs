//! A release edit as it is written: the values an edit saves and the rule
//! every saved edit holds, and the edit session a library release's editor
//! opens.

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

/// A library release as an edit session opens on it: the raw form, the
/// cover, what the editor shows beside the form, and the album the release
/// is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    pub cover: Option<bae_db::ImageRef>,
    pub display: crate::ReleaseEditDisplayContext,
    pub album: DraftAlbum,
}

/// A library release's edit session as its editor shows it now: the form
/// with each change the person made in it, the release's cover as it is now,
/// the tracks beside the form, the album a save files the release under,
/// what the library holds for each artist credit the form carries, whether a
/// save would write anything the session did not open on, and why the form
/// can't be saved, when it can't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditDraft {
    pub edit: RawReleaseEdit,
    pub cover: Option<bae_db::ImageRef>,
    pub display: crate::ReleaseEditDisplayContext,
    pub album: DraftAlbum,
    pub artist_resolutions: Vec<bae_db::ResolvedCredit>,
    pub changed: bool,
    pub invalid: Option<EditValidationError>,
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
