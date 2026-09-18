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
mod field_origin;
pub use field_origin::{
    CandidateEditField, FieldClaim, FieldClaims, FieldDot, FieldOrigin, FieldOrigins,
    FieldProvenance, FieldValues,
};
mod mark;
pub use mark::{MarkKind, ReleaseMark, ReleaseMarkLine};
mod raw_release_edit;
pub use raw_release_edit::{
    CandidateDraft, CandidateTrack, EditValidationError, RawPressingEdit, RawReleaseEdit,
    RawReleaseEditOf, RawTrackEdit, TrackFileAuthor,
};
mod verification;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::{path::Path, path::PathBuf, sync::Arc};
pub use verification::{TrackVerification, Verification, VerificationSource};

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

/// Which lookup produced a stored document, and therefore which entity's id the
/// `source_release_payloads` row is keyed by.
///
/// Wider than [`Catalog`]: identifying one release fetches supporting
/// documents that belong to other entities — its release group, a Discogs
/// master — and each is keyed by the entity it describes so two releases that
/// share one never store it twice.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadSource {
    /// A MusicBrainz release, by release id.
    MusicBrainz,
    /// A MusicBrainz release group, by group id.
    MusicBrainzReleaseGroup,
    /// A Discogs release, by release id. Also where a MusicBrainz-seeded
    /// release's cross-reference lands: the id comes out of the stored
    /// MusicBrainz document's url-rels, which is where it was found in the
    /// first place.
    Discogs,
    /// A Discogs master, by master id — read out of the Discogs release
    /// document that names it.
    DiscogsMaster,
    /// The MusicBrainz release cross-linked to a *Discogs* release, keyed by the
    /// Discogs release id. Its own key is not derivable the way the reverse
    /// direction's is: MusicBrainz's URL lookup endpoint found it, and nothing
    /// in the Discogs document names it back.
    MusicBrainzDiscogsXref,
    /// A Wikidata item, by item id — read out of the url-rels of the
    /// MusicBrainz release or release group that names it.
    Wikidata,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl PayloadSource {
    /// The stored `source` column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::MusicBrainzReleaseGroup => "musicbrainz_release_group",
            Self::Discogs => "discogs",
            Self::DiscogsMaster => "discogs_master",
            Self::MusicBrainzDiscogsXref => "musicbrainz_discogs_xref",
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
            "wikidata" => Ok(Self::Wikidata),
            _ => Err(format!("unknown payload source: {s}")),
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl std::fmt::Display for PayloadSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One document a metadata lookup returned, carrying the entity it describes so
/// the store can key it without re-reading it.
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
        /// pressing row when they agree on a barcode or a catalog number;
        /// picking the row claims both, and these are the ones the draft is
        /// *not* read from. Each names a different source from the primary and
        /// from every other partner.
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
    FileTags,
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft; candidate files and
/// mapping decisions do not.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    /// One row per candidate track, dropped and fileless rows included.
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
/// The draft is part of it: a draft has one track per slot row, so a folder
/// that gained slots — a sheet bound over a one-track image — gains a blank
/// track for each.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateMappingPreparation {
    /// The draft redrawn over the reshaped slots: the tracks it had, in
    /// position and with their edits, plus a blank row for every slot the
    /// folder now has past them.
    pub draft: CandidateDraft,
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub artist_images: Vec<PreparedArtistImage>,
}

/// Provider image answers owned by the candidate metadata revision that
/// fetched them. Import reads these bytes; it never asks a provider again.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandidatePreparedAssets {
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

/// One catalog's description of a release: which catalog, its key for this
/// release, the group that release belongs to there, and the page it publishes.
///
/// A release carries a `Vec<ReleaseRecord>` — no rows means no catalog
/// describes it, one row per catalog that does. Every row names a specific
/// pressing: picking a release is a claim about that pressing, and there is no
/// album-only claim to record.
///
/// At commit, each element becomes one record row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReleaseRecord {
    pub catalog: Catalog,
    pub key: String,
    /// The group this release belongs to in that catalog — a MusicBrainz
    /// release group, a Discogs master. A release the catalog did not group
    /// stands as its own group, which is what a catalog that groups nothing
    /// says about every release it lists: cross-catalog album merging matches
    /// on `(catalog, group)`, so such a release merges only with itself.
    pub group_key: String,
    /// The page the catalog publishes for this release. Built here, at the one
    /// place that knows a catalog's address shapes, so no surface builds one.
    pub url: String,
    /// True for the one record the draft's facts were read from, and for no
    /// other record of the same release.
    pub reads_draft: bool,
}

impl ReleaseRecord {
    /// The record for `release`, with its page built from the catalog's address
    /// shape.
    pub fn new(release: &MetadataRef, group_key: Option<String>, reads_draft: bool) -> Self {
        Self {
            catalog: release.catalog,
            key: release.key.clone(),
            // A release its catalog did not group is its own group.
            group_key: group_key.unwrap_or_else(|| release.key.clone()),
            url: release.catalog.release_url(&release.key),
            reads_draft,
        }
    }

    /// This record's release, as the key into the archived documents.
    pub fn release_ref(&self) -> MetadataRef {
        MetadataRef::new(self.catalog, self.key.clone())
    }
}

/// A new catalog release chosen for a release already in the library.
///
/// - **ExternalRelease** — "this IS my pressing." The record carries
///   `key = release_ref.key`, pressing-level metadata (year, format, label,
///   catalog number, country) seeds from the picked release, and the release
///   records that exact external provenance.
/// - **FileTags** — no catalog claim. No records, File Tags
///   provenance, and a fresh album. Metadata seeds from embedded file tags.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseReseed {
    ExternalRelease {
        release_ref: MetadataRef,
        /// The other sources' releases the picked pressing paired with — the
        /// same claim [`MetadataProvenance`] records for an import candidate.
        partners: Vec<MetadataRef>,
    },
    FileTags,
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
            Self::FileTags => MetadataProvenance::FileTags,
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
/// release's metadata provenance are untouched. So are the archived provider
/// documents, so a later re-projection can still re-seed from what the source
/// said.
///
/// For a release already in the library, `tracks` MUST have the same length as
/// the release's existing tracks; that editor cannot add or remove tracks
/// (that's a re-import, not an edit). An import's `tracks` are its track slots
/// instead, so they may outnumber the source's tracklist (audio it does not
/// account for) or fall short of it (a track no audio backs).
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
    /// Where each album-level value above came from, where the surface that
    /// built the edit knows. A surface that states nothing leaves every field
    /// unclaimed, and the write reads the fields it changes as typed.
    pub origins: FieldOrigins,
}

/// Per-pressing fields a release carries. Grouped because they share one
/// identity-claim rule: either all six come from a picked release, or the user
/// starts with all six blank and fills in what they know (File Tags or direct
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
    /// claimed a specific pressing yet (File Tags and direct-entry imports).
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
/// never by absolute path. A binding is decided when a release is picked and
/// resolved against the persisted scan candidate at commit after its physical
/// file identities have been validated.
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
    pub side: i32,
    pub track_number: Option<i32>,
    pub artist_assignments: TrackArtistAssignments,
    /// Which audio holds this track's samples, when a slot bound one to it.
    ///
    /// An import's rows are the track slots the user saw, so a pairing they
    /// corrected commits as they left it instead of being re-derived by
    /// position; a row left with no audio has nothing to write and does not
    /// become a track. The library's metadata editor never re-binds files, so
    /// every row it produces carries `None` and the release's existing
    /// bindings stand.
    pub file: Option<AudioFile>,
}

/// The current raw edit form for a library release, together with whether its
/// stored metadata provenance can be projected again. Source-less releases
/// have no source payload to project; File Tags and external releases do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    /// One entry per album-level field: where the release's value came from,
    /// what every catalog describing it says, and what its dot says.
    pub field_provenance: Vec<FieldProvenance>,
    pub can_reset_to_source: bool,
    pub cover: Option<crate::album_detail::ImageRef>,
    pub display: crate::album_detail::ReleaseEditDisplayContext,
}

/// A release's form as its source states it again, with what describes each of
/// its fields — what a reset hands the editor. The reset writes nothing, so
/// the cover and the read-only context the sheet already holds stay as they
/// are and only these two change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFormReset {
    pub edit: RawReleaseEdit,
    pub field_provenance: Vec<FieldProvenance>,
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
    /// The same edit with every field it states read from `origin`, and
    /// nothing said about the fields it leaves blank — what a projection of
    /// one source produces.
    pub fn read_from(mut self, origin: FieldOrigin) -> Self {
        self.origins = FieldOrigins::of(&FieldValues::of_edit(&self), origin);
        self
    }

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

/// Maps a logical track to the audio file that contains its samples.
///
/// Each variant owns its `DbTrack` by value — a `TrackFile` IS the track's
/// representation during import, so there is no parallel `Vec<DbTrack>`.
///
/// Standalone tracks own their file outright ("01.flac", "02.flac"). CUE-backed
/// tracks share one container file and identify themselves by their position
/// inside the CUE sheet; every CUE-backed track from one container references
/// the same `CueFlacAnalysis`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub enum TrackFile {
    Standalone {
        db_track: DbTrack,
        file_path: PathBuf,
        source_audio: crate::import::folder_scanner::ScannedAudio,
    },
    CueBacked {
        db_track: DbTrack,
        file_path: PathBuf,
        cue_pair: Arc<CueFlacAnalysis>,
        cue_index: usize,
    },
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl TrackFile {
    pub fn db_track(&self) -> &DbTrack {
        match self {
            Self::Standalone { db_track, .. } | Self::CueBacked { db_track, .. } => db_track,
        }
    }

    pub fn file_path(&self) -> &Path {
        match self {
            Self::Standalone { file_path, .. } | Self::CueBacked { file_path, .. } => file_path,
        }
    }
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
