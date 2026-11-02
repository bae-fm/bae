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
use crate::catalog_model::credits::{ArtistCredit, CreditedRole};
#[cfg(desktop)]
use crate::catalog_model::records::{MetadataRef, ReleaseRecord};
use crate::catalog_model::works::WorkGraph;
#[cfg(desktop)]
use crate::cue_flac::CueSheet;
#[cfg(desktop)]
use crate::db::DbTrack;
#[cfg(desktop)]
use bae_audio::ProbeResult;
#[cfg(desktop)]
use bae_catalogs::catalog::Catalog;
use serde::{Deserialize, Serialize};
#[cfg(desktop)]
mod progress;
#[cfg(desktop)]
pub use progress::*;
mod candidate_edit_field;
pub use candidate_edit_field::{CandidateEditField, DraftFieldEdit, PressingFactEdit};
mod artist_assignment;
pub use artist_assignment::{
    artists_standing, credited_line, ArtistAssignment, ArtistStanding, ArtistsStanding,
    CreditResolution, Credited, ExistingArtist, ResolvedCredit,
};
mod raw_release_edit;
pub use raw_release_edit::{
    EditValidationError, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
};
mod candidate_draft;
pub use candidate_draft::{
    CandidateDraft, CandidateTrack, DraftTrack, LibraryDraft, LibraryTrack, ReleaseDraft,
    TrackBinding,
};
#[cfg(desktop)]
use std::{path::PathBuf, sync::Arc};

/// What the current draft's values were read from. Direct entry and a
/// cleared draft carry no provenance.
///
/// This is the draft's own history, not a claim about which release the
/// folder is: that claim is the candidate's [`ReleaseLink`], which no draft
/// write changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: crate::catalog_model::records::LookupRef,
    },
    FileMetadata,
}

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
    Pressing(crate::identify::PressingLink),
    Album(AlbumLink),
}

#[cfg(desktop)]
impl ReleaseLink {
    /// The pressing the link names, where it names one.
    pub fn pressing(&self) -> Option<&crate::identify::PressingLink> {
        match self {
            Self::Pressing(pressing) => Some(pressing),
            Self::Album(_) => None,
        }
    }
}

/// The album a candidate is linked to while its pressing is unknown: one
/// album per catalog that files it — a MusicBrainz release group, a Discogs
/// master — in catalog order, at least one.
///
/// It claims no pressing, not even one of the pressings it was chosen among:
/// the folder's copy may be one no catalog lists.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumLink {
    albums: Vec<MetadataRef>,
}

#[cfg(desktop)]
impl AlbumLink {
    /// The link naming `albums`, put in catalog order. `None` for no album,
    /// or for two albums of one catalog: a link names the album once per
    /// catalog.
    pub fn new(albums: impl IntoIterator<Item = MetadataRef>) -> Option<Self> {
        let mut albums: Vec<MetadataRef> = albums.into_iter().collect();
        albums
            .sort_by_key(|album| Catalog::ALL.iter().position(|catalog| *catalog == album.catalog));
        let repeated = albums.windows(2).any(|pair| pair[0].catalog == pair[1].catalog);
        (!albums.is_empty() && !repeated).then_some(Self { albums })
    }

    /// The album in each catalog, in catalog order.
    pub fn albums(&self) -> &[MetadataRef] {
        &self.albums
    }

    /// Every catalog's record of the album, in catalog order: what an import
    /// of a candidate linked to it stores.
    pub fn records(&self) -> Vec<ReleaseRecord> {
        self.albums.iter().map(ReleaseRecord::album).collect()
    }
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft; candidate files and
/// mapping decisions do not.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    /// Ordered included tracks, each with audio and a required number.
    pub draft: CandidateDraft,
    pub provenance: Option<MetadataProvenance>,
    pub cover: Option<CoverSelection>,
    pub assets: CandidatePreparedAssets,
}

/// Provider image bytes owned by the candidate metadata revision that fetched
/// them: the remote cover's. Import reads these bytes; it never asks a
/// provider for the cover again.
#[cfg(desktop)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidatePreparedAssets {
    pub remote_cover: Option<bae_catalogs::remote_image::RemoteImage>,
}

/// Discogs' answer for the picture of one artist an import is about to
/// create, fetched by that import before it writes anything. `Nothing` is a
/// real answer: Discogs has no picture of that artist.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub enum DiscogsArtistImage {
    Image {
        discogs_artist_id: String,
        /// The picture as bae stores it: the JPEG
        /// [`bae_util::stored_image::stored_jpeg`] made of what Discogs
        /// served.
        jpeg: Vec<u8>,
    },
    Nothing {
        discogs_artist_id: String,
    },
}

#[cfg(desktop)]
impl DiscogsArtistImage {
    pub fn discogs_artist_id(&self) -> &str {
        match self {
            Self::Image { discogs_artist_id, .. } | Self::Nothing { discogs_artist_id } => {
                discogs_artist_id
            }
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

/// The audio a track's samples come from.
///
/// The audio is named by its identity within the release
/// ([`ScannedFile::relative_path`](crate::import::folder_scanner::ScannedFile::relative_path)),
/// never by absolute path. The draft owns this choice independently of applied
/// metadata. Import resolves it against the persisted scan after validating the
/// physical file identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioFile {
    /// The whole file holds this one track.
    Standalone { file_id: String },
    /// One of several tracks a bound track sheet carves out of one container.
    /// `index` counts that sheet's playable tracks from zero.
    SheetSlice { file_id: String, sheet_id: String, index: u32 },
}

impl AudioFile {
    /// The audio file holding this track's samples.
    pub fn file_id(&self) -> &str {
        match self {
            Self::Standalone { file_id } | Self::SheetSlice { file_id, .. } => file_id,
        }
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
    /// [`crate::db::Database::album_records_name`]); for a library release
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
    pub cover: Option<crate::album_detail::ImageRef>,
    pub display: crate::album_detail::ReleaseEditDisplayContext,
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
    /// gates its Save button on it through [`RawReleaseEdit::shape`], and
    /// `LibraryManager::apply_release_metadata_user_edit` enforces it on the write
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

/// One import track's metadata and the audio that supplies its samples.
/// The metadata has the same shape whether the audio is a whole file or a
/// track described by a CUE sheet.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub struct TrackFile {
    pub db_track: DbTrack,
    pub audio: TrackAudio,
}

/// The resolved audio source for an import track. CUE sources share their
/// sheet's analysis and identify the playable track within it.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub enum TrackAudio {
    Standalone { file_path: PathBuf, source_audio: crate::import::folder_scanner::ScannedAudio },
    CueBacked { cue_pair: Arc<CueFlacAnalysis>, cue_index: usize },
}

/// Parsed CUE sheet plus probed container analysis, shared across all tracks
/// that live inside the same file.
#[cfg(desktop)]
#[derive(Debug)]
pub struct CueFlacAnalysis {
    pub cue_sheet: CueSheet,
    pub audio_files: Vec<CueAnalyzedAudioFile>,
}

#[cfg(desktop)]
impl CueFlacAnalysis {
    /// The audio a `FILE` directive of the sheet names, as the scan resolved
    /// and probed it.
    pub(crate) fn audio_file(&self, file_reference: &str) -> Option<&CueAnalyzedAudioFile> {
        self.audio_files.iter().find(|file| file.file_reference == file_reference)
    }
}

#[cfg(desktop)]
#[derive(Debug)]
pub struct CueAnalyzedAudioFile {
    pub file_reference: String,
    pub path: PathBuf,
    pub probe: ProbeResult,
}

/// Import command sent to the service worker.
///
/// Carries only identifiers, never provider payloads. The worker consumes the
/// candidate's stored draft and its prepared cover bytes; the one provider
/// work it does is fetching the Discogs pictures of the artists it creates.
///
#[derive(Debug)]
#[cfg(desktop)]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: crate::import::CandidateKey,
    pub source: super::release_candidate::CandidateSource,
    #[cfg(any(test, feature = "test-utils"))]
    pub selected_cover: Option<CoverSelection>,
    pub destination: ImportDestination,
    /// The release a test import picks for the candidate first.
    #[cfg(any(test, feature = "test-utils"))]
    pub pick: Option<crate::identify::PressingLink>,
    /// Whether a test import then reads the draft from the files' tags.
    #[cfg(any(test, feature = "test-utils"))]
    pub read_file_tags: bool,
    #[cfg(any(test, feature = "test-utils"))]
    pub user_edit: Option<ReleaseUserEdit>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
