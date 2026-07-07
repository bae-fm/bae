//! Import type definitions
//!
//! # Import Architecture
//!
//! All imports follow the same data flow, regardless of whether tracks are stored as
//! individual files (one-file-per-track) or as a single file with a CUE sheet (one-file-per-album):
//!
//! ## Phase 1: Track-to-File Mapping (Validation)
//! - Map logical tracks (from Discogs metadata) to physical audio files (from user's folder)
//! - Validates that the user's files match the expected album structure
//! - For one-file-per-track: each logical track maps to its own file (01.flac, 02.flac, etc.)
//!   and emits `TrackFile::Standalone`
//! - For CUE/FLAC: the mapper parses the CUE sheet, probes the audio container, and emits
//!   `TrackFile::CueBacked` carrying the shared analysis
//! - Output: `Vec<TrackFile>`
//!
//! ## Phase 2: File Storage
//! - Store files to local disk or cloud storage
//! - Optionally encrypt files before storage
//! - Track progress by bytes written
//!
//! ## Phase 3: Metadata Persistence
//! - Store file records with storage paths
//! - Store track audio format records
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

    /// External URL for a specific release on this source.
    pub fn release_url(&self, release_id: &str) -> String {
        match self {
            Self::MusicBrainz => format!("https://musicbrainz.org/release/{release_id}"),
            Self::Discogs => format!("https://www.discogs.com/release/{release_id}"),
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

/// Where a release's metadata was seeded from. Pairs the source enum
/// with its release_id so the invariant "release_id is None iff source
/// is FileTags" is structural rather than a runtime check.
///
/// Maps to the two `releases` columns `metadata_source` /
/// `metadata_source_release_id`. Used as input to `set_identity` —
/// `IdentityChoice` plays the same role at import time, but it also
/// discriminates between Exact / Approximate (which `set_identity`
/// does not need: identity rows are passed in directly).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MetadataPointer {
    External {
        source: MetadataSource,
        release_id: String,
    },
    FileTags,
}

/// User's identity claim from the import flow. The variant carries
/// the release reference for the identified branches (Exact /
/// Approximate); Unknown carries none — the import seeds from
/// embedded file tags rather than a source release.
///
/// - **Exact** — "this IS my pressing." Identity row carries
///   `source_release_id = Some(release_ref.id)`. Pressing-level
///   metadata (year, format, label, catalog number, country) seeds
///   from the picked release.
/// - **Approximate** — "this is my album, but I don't claim a
///   specific pressing." Identity row carries
///   `source_release_id = None`. Pressing-level metadata stays NULL —
///   the user explicitly didn't claim a specific pressing.
///   Album-group-stable fields (title, artist, track listing) still
///   seed from `release_ref`.
/// - **Unknown** — no identity claim. Zero `release_identities` rows
///   are written. `metadata_source` is `'file_tags'`,
///   `metadata_source_release_id` is `NULL`, and the release always
///   gets a fresh album.
///
/// For Exact and Approximate, `metadata_source_release_id` on the
/// release row carries `release_ref.id` — the release records which
/// source release seeded it regardless of identity claim.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentityChoice {
    Exact { release_ref: MetadataRef },
    Approximate { release_ref: MetadataRef },
    Unknown,
}

/// User-supplied metadata edits for a release. Carries every field the
/// edit-metadata sheet is allowed to change. Plain values, not
/// row IDs — the apply layer resolves artist names against the artist
/// table and zips track edits to existing track IDs in order.
///
/// Identity rows on `release_identities`, `metadata_source`, and
/// `metadata_source_release_id` are out of scope: edits are orthogonal
/// to identity. The apply layer also leaves `release_metadata` rows
/// untouched, so a later re-projection can still re-seed from the
/// original source payload.
///
/// `tracks` MUST have the same length as the release's existing tracks;
/// edits cannot add or remove tracks (that's a re-import, not an edit).
///
/// `album_artist_names` is a positional list — element 0 is the primary
/// album artist, subsequent elements get progressively higher
/// `album_artists.position`. Empty is a validation error: every album
/// has at least one artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUserEdit {
    pub album_title: String,
    pub album_artist_names: Vec<String>,
    pub pressing: PressingEdit,
    pub tracks: Vec<TrackUserEdit>,
}

/// Per-pressing fields a release carries. Grouped because they share an
/// identity-claim semantic — either all six come from a specific picked
/// release (Exact) or the user starts with all six blank and fills in
/// what they know (Approximate / Unknown). Per-field `Option` inside
/// means "this individual field isn't known yet" within whichever case
/// the editor is in; the whole-block "no pressing claim" lives at the
/// `PressingEdit::blank()` level instead of duplicating None per field
/// at every caller.
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
}

/// Raw edit-metadata form values, exactly as the editor holds them — text
/// the user typed, not yet normalized. Artist fields are the
/// comma-separated text the user sees; pressing fields are raw strings
/// (empty means "not set"); `year` is text (parsed at shape time).
/// [`shape`](RawReleaseEdit::shape) trims, splits, parses, and validates
/// this into a wire [`ReleaseUserEdit`].
///
/// The editor binds directly to this shape: it collects raw text and asks
/// bae-core for (a) whether the form is savable and (b) the commit
/// payload, both via `shape`. The reverse projection
/// [`from_user_edit`](RawReleaseEdit::from_user_edit) seeds this form from
/// a wire edit.
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

impl RawReleaseEdit {
    /// Normalize and validate this raw form into a wire
    /// [`ReleaseUserEdit`]: trim the album title, comma-split artist
    /// fields (dropping empties), parse the year, and map empty pressing
    /// fields to `None`. `Ok` means the form is savable; the editor uses
    /// the same call to gate its Save button and to build the commit
    /// payload.
    ///
    /// Validation: album title must be non-empty after trimming, the album
    /// must resolve to at least one artist, and a non-empty year must
    /// parse as an integer (empty year is allowed — it's "not set").
    pub fn shape(&self) -> Result<ReleaseUserEdit, EditValidationError> {
        let album_title = self.album_title.trim().to_string();
        if album_title.is_empty() {
            return Err(EditValidationError::EmptyAlbumTitle);
        }

        let album_artist_names = split_artists(&self.album_artist_text);
        if album_artist_names.is_empty() {
            return Err(EditValidationError::NoAlbumArtist);
        }

        let pressing = self.pressing.shape()?;

        let tracks = self
            .tracks
            .iter()
            .map(|t| TrackUserEdit {
                title: t.title.trim().to_string(),
                side: t.side,
                track_number: t.track_number,
                artist_names: split_artists(&t.artist_text),
            })
            .collect();

        Ok(ReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing,
            tracks,
        })
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
            .map(|(index, t)| RawTrackEdit {
                id: format!("{track_id_prefix}-{index}"),
                title: t.title,
                artist_text: join_artists(&t.artist_names),
                side: t.side,
                track_number: t.track_number,
            })
            .collect();

        Self {
            album_title: edit.album_title,
            album_artist_text: join_artists(&edit.album_artist_names),
            pressing: RawPressingEdit::from_pressing(&edit.pressing),
            tracks,
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
    fn from_pressing(pressing: &PressingEdit) -> Self {
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
/// transitions to the cloud in the background (the same path `do_make_remote` runs).
///
/// Pin-vs-cloud-only is NOT part of this state: pinned-ness is coven cache state,
/// never a bae property. The user's pin choice rides the remote transition as a
/// transient argument (`pin` on the import command), threaded into the upload so
/// coven knows whether to populate `storage/pinned/`; it is never persisted here.
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
    /// Phase 0: Preparation steps (emitted from ImportHandle before pipeline starts)
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
        id: String,
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

/// Steps during phase 0 preparation (in ImportHandle, before pipeline starts)
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
/// Each variant owns its `DbTrack` by value — the TrackFile IS the track's
/// representation during import. There is no parallel `Vec<DbTrack>` alongside
/// `Vec<TrackFile>`; the DB record (title, side, duration, etc.) lives here.
///
/// Standalone tracks own their file outright (e.g., "01.flac", "02.flac").
/// CUE-backed tracks share a single container file (FLAC or APE) and identify
/// themselves by their position inside the CUE sheet. Every CUE-backed track
/// from one container references the same `CueFlacAnalysis`.
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
    pub analysis: CueAudioAnalysis,
}

#[derive(Debug)]
pub struct CueAudioAnalysis {
    pub probe: ProbeResult,
}

/// A file discovered during folder scan.
///
/// `path` is the absolute location on disk — used for reading bytes.
/// `relative_path` is the file's identity within the release (e.g. `CD1/CDImage.ape`)
/// — used as the HashMap key throughout the import pipeline and as
/// `DbFile.original_filename`. Two files may share a bare filename; they never
/// share a relative_path within one release.
// Only the desktop import pipeline (folder scan → commit) constructs these.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub size: u64,
}

/// Import command sent to the service worker.
///
/// Carries only identifiers, not payloads. For `IdentityChoice::Exact`
/// and `Approximate`, the worker calls `prepare_release` itself at
/// commit time, which transparently hits the session-wide MB/Discogs
/// LRU caches. Cache hit is the normal case (the UI's prefetch warmed
/// it); a miss costs one network round-trip but does not break
/// correctness. Cover bytes flow through the same cache pattern via
/// `download_cover_art_bytes`.
///
/// `identity_choice` carries both the user's claim shape and the
/// release reference (when applicable). For Unknown, the worker
/// sources the release shape from the scanned candidate: CUE sheets for
/// CUE-backed candidates, embedded tags for per-track-file candidates.
///
/// `user_edit` is an optional overlay from the confirmation-page
/// editor. When present, fields override the seeded metadata after the
/// choice transformation.
#[derive(Debug)]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: String,
    pub folder: PathBuf,
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
                },
                TrackUserEdit {
                    title: "Track Two".to_string(),
                    side: 2,
                    track_number: Some(1),
                    artist_names: vec![],
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
    fn release_and_group_urls_are_source_specific() {
        assert_eq!(
            MetadataSource::MusicBrainz.release_url("rel-1"),
            "https://musicbrainz.org/release/rel-1"
        );
        assert_eq!(
            MetadataSource::MusicBrainz.group_url("rg-1"),
            "https://musicbrainz.org/release-group/rg-1"
        );
        assert_eq!(
            MetadataSource::Discogs.release_url("42"),
            "https://www.discogs.com/release/42"
        );
        assert_eq!(
            MetadataSource::Discogs.group_url("master-7"),
            "https://www.discogs.com/master/master-7"
        );
    }
}
