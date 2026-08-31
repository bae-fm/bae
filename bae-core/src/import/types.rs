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
mod progress;
pub use progress::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::{path::Path, path::PathBuf, sync::Arc};

/// Metadata source for a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataSource {
    MusicBrainz,
    Discogs,
}

impl MetadataSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::Discogs => "discogs",
        }
    }

    /// Human-readable source name for user-facing copy.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "MusicBrainz",
            Self::Discogs => "Discogs",
        }
    }

    /// Human-readable name of the service a cover image came from.
    /// MusicBrainz release covers are served by its sister project, the
    /// Cover Art Archive, so the cover label differs from `display_name`.
    pub fn cover_source_label(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "Cover Art Archive",
            Self::Discogs => "Discogs",
        }
    }

    /// External URL for a release group on this source — a release-group on
    /// MusicBrainz, a master on Discogs.
    pub fn group_url(&self, group_id: &str) -> String {
        match self {
            Self::MusicBrainz => format!("https://musicbrainz.org/release-group/{group_id}"),
            Self::Discogs => format!("https://www.discogs.com/master/{group_id}"),
        }
    }
}

impl std::str::FromStr for MetadataSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "musicbrainz" => Ok(Self::MusicBrainz),
            "discogs" => Ok(Self::Discogs),
            _ => Err(format!("unknown metadata source: {s}")),
        }
    }
}

/// Which lookup produced a stored document, and therefore which entity's id the
/// `source_release_payloads` row is keyed by.
///
/// Wider than [`MetadataSource`]: identifying one release fetches supporting
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
        }
    }

    /// The payload holding a release's own editorial metadata on `source` — the
    /// anchor of everything else identification fetched alongside it.
    pub fn release_of(source: MetadataSource) -> Self {
        match source {
            MetadataSource::MusicBrainz => Self::MusicBrainz,
            MetadataSource::Discogs => Self::Discogs,
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

impl std::fmt::Display for MetadataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A source-tagged identifier into a metadata system. Whether this points at a
/// release vs. a release-group/master is determined by the field this value
/// lives in — there's no structural difference, both are `(id, source)`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataRef {
    pub id: String,
    pub source: MetadataSource,
}

/// Where the current candidate metadata draft began. Direct entry and a
/// cleared draft carry no provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataProvenance {
    ExternalRelease {
        source: MetadataSource,
        release_id: String,
    },
    FileTags,
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft; candidate files and
/// mapping decisions do not.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    pub edit: RawReleaseEdit,
    /// Complete physical bindings for `edit`: one row for every candidate
    /// track, including rows deliberately dropped or left without audio.
    pub track_mappings: Vec<crate::import::CandidateTrackMappingEdit>,
    /// Provider artists present in the selected release payload, including
    /// role and work credits that are not editable album/track assignments.
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub provenance: Option<MetadataProvenance>,
    pub cover: Option<CoverSelection>,
    pub assets: CandidatePreparedAssets,
}

/// The portion of a prepared candidate that changes when file roles or sheet
/// bindings reshape its physical track slots.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateMappingPreparation {
    pub track_mappings: Vec<crate::import::CandidateTrackMappingEdit>,
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl MetadataRef {
    pub fn new(id: impl Into<String>, source: MetadataSource) -> Self {
        Self {
            id: id.into(),
            source,
        }
    }
}

/// A single source's claim about which release this is. A release in
/// memory carries a `Vec<ReleaseIdentity>` — zero rows means no external identity
/// (no identity claim), one row per source for identified releases.
///
/// Every row names a specific pressing within its group: picking a release is
/// a claim about that pressing, and there is no album-only claim to record.
///
/// At commit, each element becomes one row in `release_identities`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReleaseIdentity {
    pub source: MetadataSource,
    pub source_group_id: String,
    pub source_release_id: String,
}

/// A new metadata source chosen for a release already in the library.
///
/// - **ExternalRelease** — "this IS my pressing." The identity row carries
///   `source_release_id = release_ref.id`, and pressing-level metadata (year,
///   format, label, catalog number, country) seeds from the picked release,
///   and the release records that exact external provenance.
/// - **FileTags** — no identity claim. Zero `release_identities` rows, File
///   Tags provenance, and a fresh album. Metadata seeds from embedded file
///   tags.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseReseed {
    ExternalRelease { release_ref: MetadataRef },
    FileTags,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ReleaseReseed {
    pub fn metadata_provenance(&self) -> MetadataProvenance {
        match self {
            Self::ExternalRelease { release_ref } => MetadataProvenance::ExternalRelease {
                source: release_ref.source,
                release_id: release_ref.id.clone(),
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
/// Identity and provenance are out of scope: `release_identities` and the
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
pub struct RawReleaseEdit {
    pub album_title: String,
    pub album_artist_assignments: Vec<ArtistAssignment>,
    pub album_year: String,
    pub pressing: RawPressingEdit,
    pub tracks: Vec<RawTrackEdit>,
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

/// The current raw edit form for a library release, together with whether its
/// stored metadata provenance can be projected again. Source-less releases
/// have no source payload to project; File Tags and external releases do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSeed {
    pub edit: RawReleaseEdit,
    pub can_reset_to_source: bool,
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
pub struct RawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_assignments: TrackArtistAssignments,
    pub side: i32,
    pub track_number: Option<i32>,
    /// The audio bound to this row, carried through editing untouched. This is
    /// what makes a pairing correctable: `shape` keeps it, so what the user
    /// left in the slot table is what the commit writes.
    pub file: Option<AudioFile>,
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

/// Trim a raw pressing field; empty (after trim) becomes `None`.
fn trim_to_option(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
/// candidate's stored metadata draft, track mappings, and prepared image
/// assets. It performs no metadata-provider work.
///
#[derive(Debug)]
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: String,
    pub folder: PathBuf,
    pub scope: crate::import::folder_scanner::ReleaseFileScope,
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
