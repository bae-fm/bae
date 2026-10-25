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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::audio_codec::ProbeResult;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::cue_flac::CueSheet;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::db::DbTrack;
use serde::{Deserialize, Serialize};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod progress;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use progress::*;
mod catalog;
pub use catalog::{parse_catalog_url, Catalog, CatalogPage};
mod candidate_edit_field;
pub use candidate_edit_field::{CandidateEditField, DraftFieldEdit, PressingFactEdit};
mod artist_assignment;
pub use artist_assignment::{
    artists_standing, credited_line, ArtistAssignment, ArtistCredit, ArtistStanding,
    ArtistsStanding, CreditResolution, Credited, ExistingArtist, ResolvedCredit, SourceCredit,
};
mod raw_release_edit;
pub use raw_release_edit::{
    EditValidationError, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
};
mod candidate_draft;
pub use candidate_draft::{
    CandidateDraft, CandidateTrack, CreditedRole, DraftTrack, GraphWork, LibraryDraft,
    LibraryTrack, ReleaseDraft, TrackBinding, WorkGraph, WorkPart,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::{path::PathBuf, sync::Arc};

/// Whether a source is asked when the sources are asked together, and when it
/// is not, why not. Core's answer, so no surface re-derives "on and reachable"
/// from a preference plus a key check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAvailability {
    /// Asked.
    On,
    /// Switched off by the person.
    Off,
    /// The source needs a credential this library does not hold, so it cannot
    /// be asked whatever the preference says. The preference is kept
    /// underneath: supplying the credential restores the choice the person
    /// last made.
    NotConfigured,
}

impl SourceAvailability {
    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// One source and whether this library asks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogAvailability {
    pub catalog: Catalog,
    pub state: SourceAvailability,
}

/// The sources an availability list says to ask, in list order. What a run's
/// provider list and a search's dispatch are both a projection of.
pub fn asked_sources(sources: &[CatalogAvailability]) -> Vec<Catalog> {
    sources
        .iter()
        .filter(|entry| entry.state.is_on())
        .map(|entry| entry.catalog)
        .collect()
}

/// Whether `source` is the only one this library still asks, so switching it
/// off would leave nothing to ask.
///
/// The rule `LibraryManager::set_metadata_source_enabled` refuses on, and the
/// rule the bridge greys that source's switch out by — one function, so a
/// switch cannot move on exactly the writes core would turn down, rather than
/// on a surface's own guess at them.
pub fn is_the_only_asked_source(sources: &[CatalogAvailability], source: Catalog) -> bool {
    asked_sources(sources) == [source]
}

/// Which lookup produced a fetched document, and therefore which entity's id it
/// is keyed by.
///
/// Wider than [`Catalog`]: fetching one release fetches supporting documents
/// that belong to other entities — its release group, a Discogs master — and
/// each is keyed by the entity it describes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadSource {
    /// A MusicBrainz release, by release id.
    MusicBrainz,
    /// A MusicBrainz release group, by group id.
    MusicBrainzReleaseGroup,
    /// A Discogs release, by release id. Also where a MusicBrainz-seeded
    /// release's cross-reference lands: the id comes out of the MusicBrainz
    /// document's url-rels.
    Discogs,
    /// A Discogs master, by master id — read out of the Discogs release
    /// document that names it.
    DiscogsMaster,
    /// The MusicBrainz release cross-linked to a *Discogs* release, keyed by the
    /// Discogs release id. Its own key is not derivable the way the reverse
    /// direction's is: MusicBrainz's URL lookup endpoint found it, and nothing
    /// in the Discogs document names it back.
    MusicBrainzDiscogsXref,
    /// A uniquely cross-linked MusicBrainz group, keyed by Discogs master id.
    MusicBrainzDiscogsMasterXref,
    /// A Wikidata item, by item id — read out of the url-rels of the
    /// MusicBrainz release or release group that names it.
    Wikidata,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl PayloadSource {
    /// The stored `document` column value of a document a fetch missed.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::MusicBrainzReleaseGroup => "musicbrainz_release_group",
            Self::Discogs => "discogs",
            Self::DiscogsMaster => "discogs_master",
            Self::MusicBrainzDiscogsXref => "musicbrainz_discogs_xref",
            Self::MusicBrainzDiscogsMasterXref => "musicbrainz_discogs_master_xref",
            Self::Wikidata => "wikidata",
        }
    }

    /// The payload holding a release's own editorial metadata on `source` — the
    /// anchor of everything else identification fetched alongside it.
    pub fn release_of(catalog: Catalog) -> Self {
        match catalog {
            Catalog::MusicBrainz => Self::MusicBrainz,
            Catalog::Discogs => Self::Discogs,
            other => unreachable!("nothing fetches documents from {}", other.as_str()),
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl std::str::FromStr for PayloadSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "musicbrainz" => Ok(Self::MusicBrainz),
            "musicbrainz_release_group" => Ok(Self::MusicBrainzReleaseGroup),
            "discogs" => Ok(Self::Discogs),
            "discogs_master" => Ok(Self::DiscogsMaster),
            "musicbrainz_discogs_xref" => Ok(Self::MusicBrainzDiscogsXref),
            "musicbrainz_discogs_master_xref" => Ok(Self::MusicBrainzDiscogsMasterXref),
            "wikidata" => Ok(Self::Wikidata),
            _ => Err(format!("unknown payload source: {s}")),
        }
    }
}

/// One document a metadata lookup returned, carrying the entity it describes so
/// the walk can key it without re-reading it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePayload {
    pub source: PayloadSource,
    pub source_release_id: String,
    pub json: String,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl SourcePayload {
    pub fn new(source: PayloadSource, source_release_id: impl Into<String>, json: String) -> Self {
        Self {
            source,
            source_release_id: source_release_id.into(),
            json,
        }
    }
}

/// One catalog's key for one entity. Whether this points at a release vs. a
/// release-group/master is determined by the field this value lives in —
/// there's no structural difference, both are `(catalog, key)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetadataRef {
    pub catalog: Catalog,
    pub key: String,
}

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
        record: MetadataRef,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseLink {
    Pressing(PressingLink),
    Album(AlbumLink),
}

impl ReleaseLink {
    /// The pressing the link names, where it names one.
    pub fn pressing(&self) -> Option<&PressingLink> {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumLink {
    albums: Vec<MetadataRef>,
}

impl AlbumLink {
    /// The link naming `albums`, put in catalog order. `None` for no album,
    /// or for two albums of one catalog: a link names the album once per
    /// catalog.
    pub fn new(albums: impl IntoIterator<Item = MetadataRef>) -> Option<Self> {
        let mut albums: Vec<MetadataRef> = albums.into_iter().collect();
        albums.sort_by_key(|album| {
            Catalog::ALL
                .iter()
                .position(|catalog| *catalog == album.catalog)
        });
        let repeated = albums
            .windows(2)
            .any(|pair| pair[0].catalog == pair[1].catalog);
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

/// The pressing a candidate is linked to: the row a person picked, or
/// identification picked unattended, under every catalog's record of it that
/// the pick claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressingLink {
    /// The catalog's release the pick names first.
    pub record: MetadataRef,
    /// The other catalogs' releases the picked pressing paired with. Find
    /// online pairs a MusicBrainz release and a Discogs release into one
    /// pressing row when the evidence their records carry says they name one
    /// physical object; picking the row claims both. Each names a different
    /// source from the primary and from every other partner.
    ///
    /// This is what the person picked, not what one provider says about
    /// another — a cross-reference an editor linked stays inferred from the
    /// documents.
    pub partners: Vec<MetadataRef>,
}

impl PressingLink {
    /// Every release the link claims, the primary first.
    pub fn claimed(&self) -> impl Iterator<Item = &MetadataRef> {
        std::iter::once(&self.record).chain(&self.partners)
    }
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft; candidate files and
/// mapping decisions do not.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    /// Ordered included tracks, each with audio and a required number.
    pub draft: CandidateDraft,
    pub provenance: Option<MetadataProvenance>,
    pub cover: Option<CoverSelection>,
    /// The cover images the applied catalog release offers the picker —
    /// every claimed release's own, then their albums' — in the order it
    /// shows them (see `source_release::pick_covers`). None for a draft read
    /// from the files' tags, typed in, or linked to an album.
    pub cover_options: Vec<crate::import::cover_art::RemoteCover>,
    pub assets: CandidatePreparedAssets,
}

/// Provider image bytes owned by the candidate metadata revision that fetched
/// them: the remote cover's. Import reads these bytes; it never asks a
/// provider for the cover again.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidatePreparedAssets {
    pub remote_cover: Option<crate::import::cover_art::RemoteImage>,
}

/// Discogs' answer for the picture of one artist an import is about to
/// create, fetched by that import before it writes anything. `Nothing` is a
/// real answer: Discogs has no picture of that artist.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub enum DiscogsArtistImage {
    Image {
        discogs_artist_id: String,
        source_url: String,
        /// The picture as bae stores it: the JPEG
        /// [`crate::util::stored_image::stored_jpeg`] made of what Discogs
        /// served.
        jpeg: Vec<u8>,
    },
    Nothing {
        discogs_artist_id: String,
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl DiscogsArtistImage {
    pub fn discogs_artist_id(&self) -> &str {
        match self {
            Self::Image {
                discogs_artist_id, ..
            }
            | Self::Nothing { discogs_artist_id } => discogs_artist_id,
        }
    }
}

impl MetadataRef {
    pub fn new(catalog: Catalog, key: impl Into<String>) -> Self {
        Self {
            catalog,
            key: key.into(),
        }
    }
}

/// A known pressing or album in one catalog. Album links do not claim that
/// any particular pressing was selected. At most one record per catalog is
/// kept, and a release's list of them names the applied record — the one its
/// draft was read from, or the linked pressing — first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseRecord {
    Pressing {
        release: MetadataRef,
        album_key: Option<String>,
    },
    Album {
        album: MetadataRef,
    },
}

impl ReleaseRecord {
    pub fn new(release: &MetadataRef, album_key: Option<String>) -> Self {
        Self::Pressing {
            release: release.clone(),
            album_key,
        }
    }

    pub fn album(album: &MetadataRef) -> Self {
        Self::Album {
            album: album.clone(),
        }
    }

    pub fn catalog(&self) -> Catalog {
        match self {
            Self::Pressing { release, .. } => release.catalog,
            Self::Album { album } => album.catalog,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Self::Pressing { release, .. } => &release.key,
            Self::Album { album } => &album.key,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Pressing { release, .. } => release.catalog.release_url(&release.key),
            Self::Album { album } => album.catalog.album_url(&album.key),
        }
    }

    pub fn release_ref(&self) -> Option<&MetadataRef> {
        match self {
            Self::Pressing { release, .. } => Some(release),
            Self::Album { .. } => None,
        }
    }

    pub fn album_ref(&self) -> Option<MetadataRef> {
        match self {
            Self::Pressing {
                release, album_key, ..
            } => album_key
                .as_ref()
                .map(|key| MetadataRef::new(release.catalog, key.clone())),
            Self::Album { album } => Some(album.clone()),
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
    /// or [`crate::pressing::Pressing::blank()`] filled in with what the person knows (file
    /// metadata or direct entry).
    pub pressing: crate::pressing::Pressing,
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
    SheetSlice {
        file_id: String,
        sheet_id: String,
        index: u32,
    },
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
        let Self {
            edit,
            cover: _,
            display,
            album,
        } = self;
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
        for assignment in self
            .album_artist_assignments
            .iter()
            .chain(
                self.tracks
                    .iter()
                    .flat_map(|track| match &track.artist_assignments {
                        TrackArtistAssignments::AlbumArtists => [].as_slice(),
                        TrackArtistAssignments::Explicit(assignments) => assignments.as_slice(),
                    }),
            )
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub struct TrackFile {
    pub db_track: DbTrack,
    pub audio: TrackAudio,
}

/// The resolved audio source for an import track. CUE sources share their
/// sheet's analysis and identify the playable track within it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub enum TrackAudio {
    Standalone {
        file_path: PathBuf,
        source_audio: crate::import::folder_scanner::ScannedAudio,
    },
    CueBacked {
        cue_pair: Arc<CueFlacAnalysis>,
        cue_index: usize,
    },
}

/// Parsed CUE sheet plus probed container analysis, shared across all tracks
/// that live inside the same file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug)]
pub struct CueFlacAnalysis {
    pub cue_sheet: CueSheet,
    pub audio_files: Vec<CueAnalyzedAudioFile>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl CueFlacAnalysis {
    /// The audio a `FILE` directive of the sheet names, as the scan resolved
    /// and probed it.
    pub(crate) fn audio_file(&self, file_reference: &str) -> Option<&CueAnalyzedAudioFile> {
        self.audio_files
            .iter()
            .find(|file| file.file_reference == file_reference)
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: String,
    pub source: super::release_candidate::CandidateSource,
    #[cfg(any(test, feature = "test-utils"))]
    pub selected_cover: Option<CoverSelection>,
    pub destination: ImportDestination,
    /// The release a test import picks for the candidate first.
    #[cfg(any(test, feature = "test-utils"))]
    pub pick: Option<PressingLink>,
    /// Whether a test import then reads the draft from the files' tags.
    #[cfg(any(test, feature = "test-utils"))]
    pub read_file_tags: bool,
    #[cfg(any(test, feature = "test-utils"))]
    pub user_edit: Option<ReleaseUserEdit>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
