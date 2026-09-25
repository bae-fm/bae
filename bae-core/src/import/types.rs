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
//! An import always lands as a local, playable release. A [`StorageMode::Remote`]
//! import then uploads to the cloud in the background.
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
pub use candidate_edit_field::CandidateEditField;
mod raw_release_edit;
pub use raw_release_edit::{
    CandidateDraft, CandidateTrack, EditValidationError, RawPressingEdit, RawReleaseEdit,
    RawReleaseEditOf, RawTrackEdit,
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

/// Where the current candidate metadata draft began. Direct entry and a
/// cleared draft carry no provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: MetadataRef,
        /// The other catalogs' releases the picked pressing paired with. Find
        /// online pairs a MusicBrainz release and a Discogs release into one
        /// pressing row when the evidence their records carry says they name
        /// one physical object; picking the row claims both, and these are
        /// the ones the draft is *not* read from. Each names a different
        /// source from the primary and from every other partner.
        ///
        /// This is what the person picked, not what one provider says about
        /// another — a cross-reference an editor linked stays inferred from
        /// the documents.
        ///
        /// Only an import candidate stores these. A library release records
        /// what its pick claimed as one record per catalog, so
        /// reading a release's provenance back names its anchor document
        /// alone.
        partners: Vec<MetadataRef>,
    },
    FileMetadata,
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft; candidate files and
/// mapping decisions do not.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    /// Ordered included tracks, each with audio and a required number.
    pub draft: CandidateDraft,
    /// Provider artists present in the selected release payload, including
    /// role and work credits that are not editable album/track assignments.
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub provenance: Option<MetadataProvenance>,
    pub cover: Option<CoverSelection>,
    pub assets: CandidatePreparedAssets,
}

/// The portion of a prepared candidate that changes when file roles or sheet
/// bindings reshape its physical track slots.
///
/// Replacing audio replaces its draft rows; unaffected tracks retain their
/// identities and metadata.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateMappingPreparation {
    /// Retained tracks and newly initialized tracks in audio order.
    pub draft: CandidateDraft,
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub artist_images: Vec<PreparedArtistImage>,
}

/// Provider image answers owned by the candidate metadata revision that
/// fetched them. Import reads these bytes; it never asks a provider again.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidatePreparedAssets {
    pub applied_source: Option<crate::import::source_release::AppliedSource>,
    pub remote_cover: Option<crate::import::cover_art::RemoteImage>,
    pub artist_images: Vec<PreparedArtistImage>,
}

/// Discogs' complete image answer for one artist currently referenced by the
/// candidate draft. `Nothing` is a real answer and prevents a later import
/// from treating absence as unfinished preparation.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub enum PreparedArtistImage {
    Image {
        discogs_artist_id: String,
        source_url: String,
        image: crate::import::cover_art::RemoteImage,
    },
    Nothing {
        discogs_artist_id: String,
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl PreparedArtistImage {
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
/// any particular pressing was selected. At most one record per catalog is kept.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseRecord {
    Pressing {
        release: MetadataRef,
        album_key: Option<String>,
        reads_draft: bool,
    },
    Album {
        album: MetadataRef,
    },
}

impl ReleaseRecord {
    pub fn new(release: &MetadataRef, album_key: Option<String>, reads_draft: bool) -> Self {
        Self::Pressing {
            release: release.clone(),
            album_key,
            reads_draft,
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

    pub fn reads_draft(&self) -> bool {
        matches!(
            self,
            Self::Pressing {
                reads_draft: true,
                ..
            }
        )
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

/// A new catalog release chosen for a release already in the library.
///
/// - **ExternalRelease** — "this IS my pressing." The record carries
///   `key = release_ref.key`, pressing-level metadata (year, format, label,
///   catalog number, country) seeds from the picked release, and the release
///   records that exact external provenance.
/// - **FileMetadata** — no catalog claim. No records, file-metadata
///   provenance, and a fresh album. Metadata seeds from what the folder's own
///   files, sheets and name say.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseReseed {
    ExternalRelease {
        release_ref: MetadataRef,
        /// The other sources' releases the picked pressing paired with — the
        /// same claim [`MetadataProvenance`] records for an import candidate.
        partners: Vec<MetadataRef>,
    },
    FileMetadata,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ReleaseReseed {
    pub fn metadata_provenance(&self) -> MetadataProvenance {
        match self {
            Self::ExternalRelease {
                release_ref,
                partners,
            } => MetadataProvenance::ExternalRelease {
                record: release_ref.clone(),
                partners: partners.clone(),
            },
            Self::FileMetadata => MetadataProvenance::FileMetadata,
        }
    }
}

/// One artist selected for album or track credit.
///
/// Existing artists stay linked by their library ID. New artists carry the
/// metadata needed to create them; source IDs are retained so commit can join
/// an external credit to an existing library artist by an exact ID match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtistAssignment {
    Existing { artist: ExistingArtist },
    New { seed: NewArtistSeed },
}

/// One artist already in the library, with the fields an editor needs to show
/// and distinguish the selection. Candidate storage persists only `artist_id`;
/// loading the candidate resolves the rest from the canonical artist row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingArtist {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

impl From<crate::db::DbArtist> for ExistingArtist {
    fn from(artist: crate::db::DbArtist) -> Self {
        Self {
            artist_id: artist.id,
            name: artist.name,
            sort_name: artist.sort_name,
            musicbrainz_artist_id: artist.musicbrainz_artist_id,
            discogs_artist_id: artist.discogs_artist_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewArtistSeed {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

/// Whether a track inherits its album artists or has its own ordered credits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackArtistAssignments {
    AlbumArtists,
    Explicit(Vec<ArtistAssignment>),
}

/// Every field the edit-metadata sheet may change. Artist choices preserve
/// whether the person selected a library artist or entered a new one; commit
/// never guesses that relationship from a name.
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
/// `album_artist_assignments` is positional — element 0 is the primary album
/// artist, later elements get progressively higher `album_artists.position`.
/// Empty is a validation error: every album has at least one artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUserEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<ArtistAssignment>,
    pub album_year: Option<i32>,
    pub pressing: PressingEdit,
    pub tracks: Vec<TrackUserEdit>,
}

/// Per-pressing fields a release carries. Grouped because they share one
/// identity-claim rule: either all six come from a picked release, or the user
/// starts with all six blank and fills in what they know (file metadata or direct
/// entry).
/// A per-field `None` means "not known yet" within whichever case the editor
/// is in; the whole-block "no pressing claim" is [`PressingEdit::blank()`], so
/// no caller has to spell out six `None`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressingEdit {
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

impl PressingEdit {
    /// All fields `None`. Pre-fill for editors where the user hasn't
    /// claimed a specific pressing yet (file metadata and direct-entry imports).
    pub fn blank() -> Self {
        Self {
            year: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        }
    }
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
    pub side: Option<i32>,
    pub track_number: Option<i32>,
    pub artist_assignments: TrackArtistAssignments,
    /// The audio chosen for an import track. Import drafts require it.
    /// The library metadata editor uses `None` because it retains the stored
    /// audio without changing it.
    pub file: Option<AudioFile>,
}

/// The current raw edit form for a library release, together with whether its
/// stored metadata provenance can be projected again. Source-less releases
/// have no source payload to project; file metadata and external releases do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    pub can_reset_to_source: bool,
    pub cover: Option<crate::album_detail::ImageRef>,
    pub display: crate::album_detail::ReleaseEditDisplayContext,
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
        self.album_artist_assignments = self
            .album_artist_assignments
            .into_iter()
            .map(ArtistAssignment::normalized)
            .collect();
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
            if assignment.is_blank() {
                return Err(EditValidationError::EmptyArtistName);
            }
        }
        Ok(())
    }
}

impl ArtistAssignment {
    pub fn new(name: impl Into<String>) -> Self {
        Self::New {
            seed: NewArtistSeed {
                name: name.into(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
        }
    }

    pub fn existing(artist: ExistingArtist) -> Self {
        Self::Existing { artist }
    }

    fn normalized(self) -> Self {
        match self {
            Self::Existing { artist } => Self::Existing { artist },
            Self::New { seed } => Self::New {
                seed: NewArtistSeed {
                    name: seed.name.trim().to_string(),
                    sort_name: seed.sort_name.and_then(|value| trim_to_option(&value)),
                    musicbrainz_artist_id: seed
                        .musicbrainz_artist_id
                        .and_then(|value| trim_to_option(&value)),
                    discogs_artist_id: seed
                        .discogs_artist_id
                        .and_then(|value| trim_to_option(&value)),
                },
            },
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        match self {
            Self::Existing { artist } => {
                artist.artist_id.trim().is_empty() || artist.name.trim().is_empty()
            }
            Self::New { seed } => seed.name.trim().is_empty(),
        }
    }
}

impl TrackArtistAssignments {
    fn normalize(&mut self) {
        if let Self::Explicit(assignments) = self {
            *assignments = std::mem::take(assignments)
                .into_iter()
                .map(ArtistAssignment::normalized)
                .collect();
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
        file_path: PathBuf,
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
#[derive(Debug)]
pub struct CueAnalyzedAudioFile {
    pub file_reference: String,
    pub path: PathBuf,
    pub probe: ProbeResult,
}

/// Import command sent to the service worker.
///
/// Carries only identifiers, never provider payloads. The worker consumes the
/// candidate's stored draft and prepared image assets. It performs no
/// metadata-provider work.
///
#[derive(Debug)]
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: String,
    pub source: super::release_candidate::CandidateSource,
    #[cfg(any(test, feature = "test-utils"))]
    pub selected_cover: Option<CoverSelection>,
    pub storage_mode: StorageMode,
    /// The transient pin choice for a `Remote` import: whether coven keeps
    /// the uploaded blobs in `storage/pinned/` (kept offline) vs the evictable
    /// cache. Ignored for `Local`. Never persisted — it rides the upload
    /// as the retain-pinned intent.
    pub pin: bool,
    #[cfg(any(test, feature = "test-utils"))]
    pub metadata_provenance: Option<MetadataProvenance>,
    #[cfg(any(test, feature = "test-utils"))]
    pub user_edit: Option<ReleaseUserEdit>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
