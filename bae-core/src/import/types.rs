//! Import type definitions.
//!
//! Every import follows the same flow, whether the tracks are individual files
//! or one container plus a CUE sheet, and whether the metadata came from
//! MusicBrainz, Discogs, or the files' own tags:
//!
//! 1. **Preparation** ([`PrepareStep`]) — resolve the release's metadata, walk
//!    the folder, and map each logical track onto the audio file holding its
//!    samples. A one-file-per-track release emits one `TrackFile::Standalone`
//!    per track; a CUE-backed release emits a `TrackFile::CueBacked` per track,
//!    all sharing the container's parsed CUE sheet and probe.
//! 2. **Running** ([`ImportPhase`]) — reference each file where it already sits
//!    (no bytes move, no transcode), measure per-track loudness by decoding, and
//!    write every row in one transaction.
//!
//! An import always lands as a local, playable release. A [`StorageMode::Remote`]
//! import then uploads to the cloud in the background.
use crate::audio_codec::ProbeResult;
use crate::cue_flac::CueSheet;
use crate::db::DbTrack;
use serde::{Deserialize, Serialize};
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

impl std::fmt::Display for PayloadSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One document a metadata lookup returned, carrying the entity it describes so
/// the store can key it without re-reading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePayload {
    pub source: PayloadSource,
    pub source_release_id: String,
    pub json: String,
}

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataRef {
    pub id: String,
    pub source: MetadataSource,
}

/// The identity the user chose for a folder candidate — the pressing they
/// picked and how far they claim it reaches, or the decision to read the
/// folder's own tags. Persisted on `import_candidate_state` (JSON,
/// `identity_pick`) so an answered pane reopens answered after a restart; the
/// claim line, the seed, and the mapping are all re-derived from it against the
/// archived documents, never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityPick {
    Release {
        source: MetadataSource,
        release_id: String,
        /// The pressing claim, at [`ClaimLevel::Exact`] unless the user lowered
        /// it. Stored here because it is an assertion about the record they
        /// hold, which no later derivation could recover.
        claim: ClaimLevel,
    },
    Unknown,
}

impl IdentityPick {
    /// What committing this pick records as the release's identity.
    pub fn choice(&self) -> IdentityChoice {
        match self {
            Self::Release {
                source,
                release_id,
                claim,
            } => claim.choice(MetadataRef::new(release_id.clone(), *source)),
            Self::Unknown => IdentityChoice::Unknown,
        }
    }
}

/// A candidate's decided identity with everything the pane seeds from it —
/// the one payload the pick command and the selection query both return, so
/// a fresh launch renders exactly what the click rendered. Not persisted:
/// derived from the stored [`IdentityPick`] and the archived documents on
/// every read. Desktop-only like the prefetch and mapping it carries: the
/// import pane is where a decided identity is read.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub enum DecidedIdentity {
    Release {
        source: MetadataSource,
        release_id: String,
        prefetch: crate::import::search::ImportReleasePrefetch,
    },
    /// The folder read as its own files describe it.
    Unknown {
        seed: ReleaseUserEdit,
        mapping: crate::import::mapping::MappingTable,
    },
}

impl MetadataRef {
    pub fn new(id: impl Into<String>, source: MetadataSource) -> Self {
        Self {
            id: id.into(),
            source,
        }
    }
}

/// A single source's claim about which release this is. A release in
/// memory carries a `Vec<ReleaseIdentity>` — zero rows means Unknown
/// (no identity claim), one row per source for identified releases.
///
/// `source_release_id = None` is Approximate (the claim is at the group
/// level only); `Some` is Exact (a specific pressing within the group).
///
/// At commit, each element becomes one row in `release_identities`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReleaseIdentity {
    pub source: MetadataSource,
    pub source_group_id: String,
    pub source_release_id: Option<String>,
}

/// Where a release's metadata was seeded from, for the `releases` columns
/// `metadata_source` / `metadata_source_release_id`. Pairing the source with
/// its release_id makes "release_id is None iff source is FileTags" structural
/// rather than a runtime check.
///
/// Input to `set_identity`. `IdentityChoice` plays the same role at import
/// time, but also discriminates Exact from Approximate — which `set_identity`
/// doesn't need, since it takes identity rows directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetadataPointer {
    External {
        source: MetadataSource,
        release_id: String,
    },
    FileTags,
}

/// The user's identity claim from the import flow.
///
/// - **Exact** — "this IS my pressing." The identity row carries
///   `source_release_id = Some(release_ref.id)`, and pressing-level metadata
///   (year, format, label, catalog number, country) seeds from the picked
///   release.
/// - **Approximate** — "this is my album, but I don't claim a specific
///   pressing." The identity row carries `source_release_id = None` and
///   pressing-level metadata stays NULL. Album-group-stable fields (title,
///   artist, track listing) still seed from `release_ref`.
/// - **Unknown** — no claim. Zero `release_identities` rows, `metadata_source`
///   is `'file_tags'`, `metadata_source_release_id` is NULL, and the release
///   always gets a fresh album. Metadata seeds from embedded file tags.
///
/// For Exact and Approximate, `metadata_source_release_id` on the release row
/// carries `release_ref.id` either way — the release records which source
/// release seeded it regardless of the claim.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentityChoice {
    Exact { release_ref: MetadataRef },
    Approximate { release_ref: MetadataRef },
    Unknown,
}

/// How far a claim on a picked release reaches — the two of [`IdentityChoice`]
/// that name a release, with the release left out.
///
/// A pick arrives at [`Self::Exact`]: clicking a release says "this is the one
/// I have", whatever turned it up. Only the user moves it, when they hold the
/// album but can't vouch for the pressing. It is stored with the pick rather
/// than derived from the evidence, because it is an assertion about the record
/// in the room that no metadata can settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimLevel {
    /// This pressing is the one in the room.
    Exact,
    /// The album, with which pressing left open.
    Approximate,
}

impl ClaimLevel {
    /// What claiming `release_ref` at this level records — what commit writes
    /// identity rows from.
    pub fn choice(self, release_ref: MetadataRef) -> IdentityChoice {
        match self {
            Self::Exact => IdentityChoice::Exact { release_ref },
            Self::Approximate => IdentityChoice::Approximate { release_ref },
        }
    }
}

/// Every field the edit-metadata sheet may change. Plain values, not row IDs —
/// the apply layer resolves artist names against the artist table and zips
/// track edits to existing track IDs in order.
///
/// Identity is out of scope: `release_identities`, `metadata_source`, and
/// `metadata_source_release_id` are untouched. So are the archived provider
/// documents, so a later re-projection can still re-seed from what the source
/// said.
///
/// For a release already in the library, `tracks` MUST have the same length as
/// the release's existing tracks; that editor cannot add or remove tracks
/// (that's a re-import, not an edit). An import's `tracks` are its track slots
/// instead, so they may outnumber the source's tracklist (audio it does not
/// account for) or fall short of it (a track no audio backs).
///
/// `album_artist_names` is positional — element 0 is the primary album artist,
/// later elements get progressively higher `album_artists.position`. Empty is a
/// validation error: every album has at least one artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUserEdit {
    pub album_title: String,
    pub album_artist_names: Vec<String>,
    pub pressing: PressingEdit,
    pub tracks: Vec<TrackUserEdit>,
}

/// Per-pressing fields a release carries. Grouped because they share one
/// identity-claim rule: either all six come from a picked release (Exact), or
/// the user starts with all six blank and fills in what they know (Approximate
/// / Unknown). A per-field `None` means "not known yet" within whichever case
/// the editor is in; the whole-block "no pressing claim" is
/// [`PressingEdit::blank()`], so no caller has to spell out six `None`s.
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
    /// claimed a specific pressing yet (Approximate / Unknown imports).
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
/// read again when the commit re-walks the folder; only the relative path
/// survives the folder being moved or renamed in between.
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
/// `artist_names` empty means the track shares the album artist (no
/// per-track artist rows written). Otherwise the list is positional —
/// element 0 is the primary track artist, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackUserEdit {
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub artist_names: Vec<String>,
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
/// typed, not yet normalized. Artist fields are the comma-separated text the
/// user sees; pressing fields are raw strings (empty means "not set"); `year`
/// is text.
///
/// The editor binds directly to this shape and calls
/// [`shape`](RawReleaseEdit::shape) both to gate its Save button and to build
/// the commit payload: it trims, splits, parses, and validates into a wire
/// [`ReleaseUserEdit`]. [`from_user_edit`](RawReleaseEdit::from_user_edit) is
/// the reverse, seeding this form from a wire edit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawReleaseEdit {
    pub album_title: String,
    /// Comma-separated artist text in positional order, as typed.
    pub album_artist_text: String,
    pub pressing: RawPressingEdit,
    pub tracks: Vec<RawTrackEdit>,
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
/// tracks zip positionally to existing track IDs. `artist_text` empty
/// means "share the album artist".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_text: String,
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
    #[error("Year must be a number")]
    InvalidYear,
}

/// Split a comma-separated artist field into trimmed names, dropping
/// empties. The inverse of `join_artists`; both must agree so a form
/// seeded from `from_user_edit` round-trips.
fn split_artists(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Join positional names into the editor's comma-separated text. Inverse
/// of `split_artists`.
fn join_artists(names: &[String]) -> String {
    names.join(", ")
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

impl ReleaseUserEdit {
    /// Trim the album title and every track title, and drop blank artist names.
    /// The normalization the editor's [`RawReleaseEdit::shape`] performs on typed
    /// text, hoisted onto the wire type so a surface that builds one
    /// field-for-field — MCP, the CLI — gets the same treatment instead of
    /// writing whatever it was handed. Idempotent.
    pub fn normalized(mut self) -> Self {
        self.album_title = self.album_title.trim().to_string();
        self.album_artist_names = normalized_names(self.album_artist_names);
        for track in &mut self.tracks {
            track.title = track.title.trim().to_string();
            track.artist_names = normalized_names(std::mem::take(&mut track.artist_names));
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
        if self.album_artist_names.iter().all(|n| n.trim().is_empty()) {
            return Err(EditValidationError::NoAlbumArtist);
        }
        Ok(())
    }
}

/// Trim each name and drop the blanks — what `split_artists` does to comma text,
/// for a list that arrived already split.
fn normalized_names(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

impl RawReleaseEdit {
    /// Normalize and validate this raw form into a wire [`ReleaseUserEdit`]:
    /// trim the album title, comma-split artist fields (dropping empties),
    /// parse the year, and map empty pressing fields to `None`. `Ok` means the
    /// form is savable.
    ///
    /// Validation: the album title must be non-empty after trimming, the album
    /// must resolve to at least one artist (both via
    /// [`ReleaseUserEdit::validate`]), and a non-empty year must parse as an
    /// integer (an empty year is allowed — it's "not set").
    pub fn shape(&self) -> Result<ReleaseUserEdit, EditValidationError> {
        let edit = ReleaseUserEdit {
            album_title: self.album_title.clone(),
            album_artist_names: split_artists(&self.album_artist_text),
            pressing: self.pressing.shape()?,
            tracks: self
                .tracks
                .iter()
                .map(|t| TrackUserEdit {
                    title: t.title.clone(),
                    side: t.side,
                    track_number: t.track_number,
                    artist_names: split_artists(&t.artist_text),
                    file: t.file.clone(),
                })
                .collect(),
        }
        .normalized();
        edit.validate()?;
        Ok(edit)
    }

    /// Seed a raw editor form from a wire [`ReleaseUserEdit`]. Joins artist
    /// lists into comma text and renders absent pressing fields as empty
    /// strings — the inverse of `shape`. `track_id_prefix` provides the
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
            album_artist_text: join_artists(&edit.album_artist_names),
            pressing: RawPressingEdit::from_pressing(&edit.pressing),
            tracks,
        }
    }
}

impl RawTrackEdit {
    /// Seed one raw editor row from a wire [`TrackUserEdit`], under the row
    /// identity `id`. Joins the artist list into comma text and carries the
    /// bound audio through untouched.
    pub fn from_user_edit(edit: TrackUserEdit, id: String) -> Self {
        Self {
            id,
            title: edit.title,
            artist_text: join_artists(&edit.artist_names),
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
        let year = match self.year.trim() {
            "" => None,
            text => Some(
                text.parse::<i32>()
                    .map_err(|_| EditValidationError::InvalidYear)?,
            ),
        };
        Ok(PressingEdit {
            year,
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

/// The storage state the user picks for an import. Every import FIRST lands
/// `Local` (files in place, playable immediately); a `Remote` import then
/// transitions to the cloud in the background.
///
/// Pinned-ness is NOT part of this state — it's coven cache state, never a bae
/// property. The user's pin choice rides the remote transition as a transient
/// argument (`pin` on the import command) telling coven whether to populate
/// `storage/pinned/`; it is never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// Files stay in place on this device; never uploaded.
    Local,
    /// Uploaded to the cloud home; `releases.remote` flips true once the upload
    /// lands.
    Remote,
}

/// User's cover art selection for an import.
#[derive(Clone, Debug)]
pub enum CoverSelection {
    /// Remote cover to download (URL + source for attribution)
    Remote(String, MetadataSource),
    /// Local file in the album folder (relative path from album root)
    Local(String),
}

/// Progress updates during import
#[derive(Debug, Clone)]
pub enum ImportProgress {
    /// A preparation step, before the running phases begin.
    Preparing {
        import_id: String,
        step: PrepareStep,
        album_title: String,
        artist_name: String,
    },
    Started {
        id: String,
        import_id: String,
    },
    Progress {
        id: String,
        percent: u8,
        /// Which running phase this progress belongs to. The phases run in order:
        /// reference the files in place, measure loudness, finalize.
        phase: ImportPhase,
        import_id: String,
    },
    Complete {
        id: String,
        import_id: String,
        album_id: String,
    },
    RemoteUploadQueued {
        id: String,
        import_id: String,
        album_id: String,
    },
    Failed {
        error: String,
        import_id: String,
    },
}

/// The running phase of an import, after phase-0 preparation. Emitted as each
/// transition begins so the UI can name the work in progress. Every import is
/// local-in-place: the source files are referenced where they sit, then each
/// track is decoded to measure loudness (the long pole), then the rows are
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportPhase {
    /// Recording each source file's row and referencing it in place. No bytes
    /// move; per-file progress fills the percent.
    ReferencingFiles,
    /// Decoding each track to measure its loudness and true peak. Per-track
    /// progress arrives on the dedicated `ImportLoudnessProgress` event, not the
    /// coarse percent.
    MeasuringLoudness,
    /// Writing the album/release/track rows and committing the import.
    Finalizing,
}

/// Preparation steps, emitted by the import worker before the running phases
/// ([`ImportPhase`]) begin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareStep {
    ParsingMetadata,
    WritingCoverArt,
    DiscoveringFiles,
    ValidatingTracks,
    SavingToDatabase,
}

/// Which step of an import is in progress, for the candidate progress UI. The
/// UI localizes each step; bae-core no longer renders display text for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    Preparing(PrepareStep),
    Running(ImportPhase),
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
#[derive(Debug, Clone)]
pub enum TrackFile {
    Standalone {
        db_track: DbTrack,
        file_path: PathBuf,
    },
    CueBacked {
        db_track: DbTrack,
        file_path: PathBuf,
        cue_pair: Arc<CueFlacAnalysis>,
        cue_index: usize,
    },
}

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
#[derive(Debug)]
pub struct CueFlacAnalysis {
    pub cue_sheet: CueSheet,
    pub audio_files: Vec<CueAnalyzedAudioFile>,
}

#[derive(Debug)]
pub struct CueAnalyzedAudioFile {
    pub file_reference: String,
    pub path: PathBuf,
    pub probe: ProbeResult,
}

/// Import command sent to the service worker.
///
/// Carries only identifiers, never payloads. For `IdentityChoice::Exact` and
/// `Approximate`, the worker calls `prepare_release` at commit time, reading
/// through the session-wide MB/Discogs LRU caches — normally a hit, since the
/// UI's prefetch warmed them; a miss costs one round-trip. Cover bytes come
/// through the same caching in the remote-image cache. For Unknown, the
/// worker sources the release shape from the scanned candidate: CUE sheets for
/// CUE-backed candidates, embedded tags for per-track-file candidates.
///
/// `user_edit` is an optional overlay from the confirmation-page editor; when
/// present its fields override the seeded metadata after the choice
/// transformation.
#[derive(Debug)]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: String,
    pub folder: PathBuf,
    pub scope: crate::import::folder_scanner::ReleaseFileScope,
    pub selected_cover: Option<CoverSelection>,
    pub storage_mode: StorageMode,
    /// The transient pin choice for a `Remote` import: whether coven keeps
    /// the uploaded blobs in `storage/pinned/` (kept offline) vs the evictable
    /// cache. Ignored for `Local`. Never persisted — it rides the upload
    /// as the retain-pinned intent.
    pub pin: bool,
    pub identity_choice: IdentityChoice,
    pub user_edit: Option<ReleaseUserEdit>,
}

#[cfg(test)]
mod edit_shaping_tests {
    use super::*;

    /// A raw form that shapes cleanly: one album artist, a track with its
    /// own artists, all pressing fields filled. The individual tests mutate
    /// one aspect to exercise a single rule.
    fn valid_form() -> RawReleaseEdit {
        RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_text: "Artist One".to_string(),
            pressing: RawPressingEdit {
                year: "1999".to_string(),
                format: "2×LP".to_string(),
                label: "Label Name".to_string(),
                catalog_number: "CAT-123".to_string(),
                country: "US".to_string(),
                barcode: "0123456789".to_string(),
            },
            tracks: vec![RawTrackEdit {
                id: "track-0".to_string(),
                title: "Track Title".to_string(),
                artist_text: "Artist Two".to_string(),
                side: 1,
                track_number: Some(1),
                file: Some(AudioFile::Standalone {
                    file_id: "01.flac".to_string(),
                }),
            }],
        }
    }

    #[test]
    fn shapes_a_valid_form_into_a_wire_edit() {
        let shaped = valid_form().shape().expect("valid form shapes");
        assert_eq!(shaped.album_title, "Album Title");
        assert_eq!(shaped.album_artist_names, vec!["Artist One".to_string()]);
        assert_eq!(shaped.pressing.year, Some(1999));
        assert_eq!(shaped.pressing.format.as_deref(), Some("2×LP"));
        assert_eq!(shaped.tracks.len(), 1);
        assert_eq!(shaped.tracks[0].title, "Track Title");
        assert_eq!(
            shaped.tracks[0].artist_names,
            vec!["Artist Two".to_string()]
        );
    }

    #[test]
    fn splits_comma_separated_artists_trimming_and_dropping_empties() {
        let mut form = valid_form();
        form.album_artist_text = " Artist One ,Artist Two,  , Artist Three ".to_string();
        let shaped = form.shape().expect("shapes");
        assert_eq!(
            shaped.album_artist_names,
            vec![
                "Artist One".to_string(),
                "Artist Two".to_string(),
                "Artist Three".to_string(),
            ]
        );
    }

    #[test]
    fn empty_track_artist_text_yields_no_track_artists() {
        let mut form = valid_form();
        form.tracks[0].artist_text = "   ".to_string();
        let shaped = form.shape().expect("shapes");
        assert!(shaped.tracks[0].artist_names.is_empty());
    }

    #[test]
    fn trims_album_title() {
        let mut form = valid_form();
        form.album_title = "  Album Title  ".to_string();
        assert_eq!(form.shape().expect("shapes").album_title, "Album Title");
    }

    #[test]
    fn empty_pressing_fields_map_to_none() {
        let mut form = valid_form();
        form.pressing = RawPressingEdit {
            year: "".to_string(),
            format: "  ".to_string(),
            label: "".to_string(),
            catalog_number: "".to_string(),
            country: "".to_string(),
            barcode: "".to_string(),
        };
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing, PressingEdit::blank());
    }

    #[test]
    fn parses_year_and_trims_pressing_fields() {
        let mut form = valid_form();
        form.pressing.year = "  2001  ".to_string();
        form.pressing.country = "  JP  ".to_string();
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing.year, Some(2001));
        assert_eq!(pressing.country.as_deref(), Some("JP"));
    }

    #[test]
    fn empty_album_title_is_a_validation_error() {
        let mut form = valid_form();
        form.album_title = "   ".to_string();
        assert_eq!(form.shape(), Err(EditValidationError::EmptyAlbumTitle));
    }

    #[test]
    fn all_empty_artist_text_is_a_validation_error() {
        let mut form = valid_form();
        form.album_artist_text = " , ,  ".to_string();
        assert_eq!(form.shape(), Err(EditValidationError::NoAlbumArtist));
    }

    #[test]
    fn unparseable_year_is_a_validation_error() {
        let mut form = valid_form();
        form.pressing.year = "19x9".to_string();
        assert_eq!(form.shape(), Err(EditValidationError::InvalidYear));
    }

    /// Seeding a form from a wire edit then shaping it back recovers the
    /// original wire edit: `from_user_edit` and `shape` are inverses.
    #[test]
    fn from_user_edit_round_trips_through_shape() {
        let original = ReleaseUserEdit {
            album_title: "Album Title".to_string(),
            album_artist_names: vec!["Artist One".to_string(), "Artist Two".to_string()],
            pressing: PressingEdit {
                year: Some(1999),
                format: Some("2×LP".to_string()),
                label: None,
                catalog_number: Some("CAT-123".to_string()),
                country: None,
                barcode: None,
            },
            tracks: vec![
                TrackUserEdit {
                    title: "Track One".to_string(),
                    side: 1,
                    track_number: Some(1),
                    artist_names: vec!["Track Artist".to_string()],
                    file: Some(AudioFile::Standalone {
                        file_id: "01.flac".to_string(),
                    }),
                },
                TrackUserEdit {
                    title: "Track Two".to_string(),
                    side: 2,
                    track_number: Some(1),
                    artist_names: vec![],
                    file: None,
                },
            ],
        };

        let raw = RawReleaseEdit::from_user_edit(original.clone(), "reset-track");
        assert_eq!(raw.album_artist_text, "Artist One, Artist Two");
        assert_eq!(raw.tracks[0].id, "reset-track-0");
        assert_eq!(raw.tracks[1].id, "reset-track-1");
        assert_eq!(raw.tracks[0].artist_text, "Track Artist");
        assert_eq!(raw.tracks[1].artist_text, "");
        assert_eq!(raw.pressing.year, "1999");
        assert_eq!(raw.pressing.label, "");

        assert_eq!(raw.shape().expect("re-shapes"), original);
    }
}

#[cfg(test)]
mod metadata_source_tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn from_str_round_trips_known_sources() {
        assert_eq!(
            MetadataSource::from_str("musicbrainz"),
            Ok(MetadataSource::MusicBrainz)
        );
        assert_eq!(
            MetadataSource::from_str("discogs"),
            Ok(MetadataSource::Discogs)
        );
        // as_str is the inverse of from_str.
        assert_eq!(
            MetadataSource::from_str(MetadataSource::MusicBrainz.as_str()),
            Ok(MetadataSource::MusicBrainz)
        );
    }

    #[test]
    fn from_str_rejects_unknown_source() {
        let err = MetadataSource::from_str("bandcamp").expect_err("unknown source should error");
        assert!(
            err.contains("unknown metadata source") && err.contains("bandcamp"),
            "unexpected error: {err}"
        );
        // The match is exact — casing isn't accepted.
        assert!(MetadataSource::from_str("MusicBrainz").is_err());
    }

    #[test]
    fn group_urls_are_source_specific() {
        assert_eq!(
            MetadataSource::MusicBrainz.group_url("rg-1"),
            "https://musicbrainz.org/release-group/rg-1"
        );
        assert_eq!(
            MetadataSource::Discogs.group_url("master-7"),
            "https://www.discogs.com/master/master-7"
        );
    }
}
