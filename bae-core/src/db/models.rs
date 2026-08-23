//! Raw DB-shape types: `Db*` rows mirroring a table row, and
//! `Db*Detail` / `Db*Summary` / `Db*SearchResult` aggregates whose shape is a
//! join query's result — stitched together in SQL, not in Rust.
//!
//! **No formatting, no Rust-side derivation, no filesystem access.** Every
//! `*_label`, sum-of-fields, group-by, path resolution, `has_X` flag, or
//! `stored.or_else(first)` fallback belongs in the manager, not here.
//!
//! `LibraryManager` in `crate::library::manager` is the resolver: it turns
//! these into the display-ready `AlbumDetail`, `ReleaseDetail`, `AlbumSummary`,
//! `ReleaseStorageSummary`, and `SearchResults` of `crate::album_detail`, which
//! is what the bridge and event payloads carry. (`crate::queue::QueueItem` is
//! built directly by `db::get_queue_items` — it has no raw counterpart here.)

use crate::import::MetadataSource;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};

mod images;
mod query;

pub use images::*;
pub use query::*;

/// An individual artist or band. Linked to albums and tracks through the
/// `album_artists` / `track_artists` junction tables, so an album or track can
/// credit several artists (collaborations, features, compilations).
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtist {
    pub id: String,
    pub name: String,
    /// Sort name for alphabetical ordering (e.g., "Artist Name, The")
    pub sort_name: Option<String>,
    /// Artist ID from Discogs (for deduplication across imports)
    pub discogs_artist_id: Option<String>,
    /// Artist ID from MusicBrainz (for deduplication across imports)
    pub musicbrainz_artist_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Well-known "Various Artists" IDs per metadata source.
pub struct VariousArtistsIds {
    pub discogs: &'static str,
    pub musicbrainz: &'static str,
}

pub const VARIOUS_ARTISTS: VariousArtistsIds = VariousArtistsIds {
    discogs: "194",
    musicbrainz: "89ad4ac3-39f7-470e-963a-56509c546377",
};

impl DbArtist {
    /// True if this artist is the well-known "Various Artists" placeholder of any
    /// metadata source. The destructure is deliberate: adding a `*_artist_id`
    /// field to `DbArtist` without handling it here is a compile error.
    pub fn is_various_artists(&self) -> bool {
        let DbArtist {
            discogs_artist_id,
            musicbrainz_artist_id,
            id: _,
            name: _,
            sort_name: _,
            created_at: _,
        } = self;

        discogs_artist_id.as_deref() == Some(VARIOUS_ARTISTS.discogs)
            || musicbrainz_artist_id.as_deref() == Some(VARIOUS_ARTISTS.musicbrainz)
    }
}

/// Links artists to albums (many-to-many).
#[derive(Debug, Clone)]
pub struct DbAlbumArtist {
    pub id: String,
    pub album_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist albums (0-indexed)
    pub position: i32,
    pub created_at: DateTime<Utc>,
}
/// Links artists to tracks (many-to-many).
#[derive(Debug, Clone)]
pub struct DbTrackArtist {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist tracks (0-indexed)
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWork {
    /// Minted, never a source id — a MusicBrainz work MBID is often a name-based
    /// (version 3) UUID, which the sync layer refuses on a synced row.
    pub id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    /// The MusicBrainz work this row came from — the only source of works — and
    /// what an import dedups on.
    pub musicbrainz_work_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkArtist {
    pub id: String,
    pub work_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkPart {
    pub id: String,
    pub parent_work_id: String,
    pub child_work_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWork {
    pub id: String,
    pub track_id: String,
    pub work_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseArtistRole {
    pub id: String,
    pub release_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackArtistRole {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// A logical album (the "master"): the metadata common to every physical release
/// of it ("1973 Original", "2016 Remaster", …). The releases themselves, and
/// their import status, live in the `releases` table.
///
/// Albums carry no per-source identity of their own — identity lives per release
/// in the `release_identities` side table, and cross-source equivalences surface
/// implicitly when an album's releases hold rows in several sources. Releases
/// attach to albums loosely (title/artist match), so album-level identity
/// columns would claim a certainty the attach rule doesn't have.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbum {
    pub id: String,
    pub title: String,
    /// FK to the primary artist. Additional artists live in album_artists junction table.
    pub artist_id: String,
    pub year: Option<i32>,
    /// User-chosen canonical release for this album. When set, this release
    /// provides the album's cover art. When None, callers fall back to the
    /// first release in `release_ids`.
    pub primary_release_id: Option<String>,
    /// True for "Various Artists" compilation albums
    pub is_compilation: bool,
    pub created_at: DateTime<Utc>,
}

pub(crate) fn resolve_primary_release_id<'a>(
    stored_primary_release_id: Option<&str>,
    release_ids: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let release_ids: Vec<&str> = release_ids.into_iter().collect();
    stored_primary_release_id
        .filter(|id| release_ids.iter().any(|release_id| release_id == id))
        .or_else(|| release_ids.first().copied())
        .map(str::to_string)
}

/// Raw album-summary aggregate: artist names and release IDs joined in SQL to
/// avoid N+1 lookups. The resolver in `LibraryManager` produces the
/// display-ready `crate::album_detail::AlbumSummary` (applying the
/// `primary_release_id` fallback).
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    pub release_ids: Vec<String>,
    /// User-chosen primary release. `None` if unset — the resolver in
    /// `LibraryManager` falls back to the first release.
    pub primary_release_id: Option<String>,
}

/// Probe whether a search-result candidate is already in the library: as the
/// same pressing (a `release_identities` row on this `source` whose
/// `source_release_id` is `release_id`), or as the same album (any release with
/// this `source` + `source_group_id`). Fields are the source's own IDs, not
/// bae's; the `LibraryStatus` result is correlated back via `release_id`.
///
/// `source_group_id` is optional because a search result may not surface a group
/// (rare for MusicBrainz, happens for Discogs releases with no master) — those
/// candidates skip the album-level lookup.
#[derive(Debug, Clone)]
pub struct LibraryCheck {
    pub release_id: String,
    pub source: MetadataSource,
    pub source_group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStatus {
    pub release_id: String,
    pub release_in_library: bool,
    pub album_in_library: bool,
    pub album_title: Option<String>,
    pub album_id: Option<String>,
}

/// A release's pressing-level editorial metadata. A substruct so "no pressing
/// claim" is one `Pressing::blank()` rather than nilling six fields at every
/// caller — see `many-fields-none-together-means-a-missing-type`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pressing {
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    /// e.g. "CD", "Vinyl", "Digital"
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

impl Pressing {
    /// All fields `None` — "user claimed an album, not a specific
    /// pressing." Used when import identity is Approximate.
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

/// A specific physical or digital version of a logical album ("1973 Original
/// Pressing", "2016 Remaster", "180g Vinyl", …). Files and tracks hang off
/// releases rather than albums: users import a specific release, each has its own
/// audio and metadata, and several releases of one album coexist in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct DbRelease {
    pub id: String,
    pub album_id: String,
    /// Human-readable release name (e.g., "2016 Remaster", "180g Vinyl"). `None`
    /// when the user never named a version.
    pub release_name: Option<String>,
    /// Pressing-level editorial metadata. `Pressing::blank()` means "user claimed
    /// an album, not a specific pressing" (Approximate imports). The DB columns
    /// stay flat (`year`, `format`, `label`, …) — this grouping is Rust-side only.
    pub pressing: Pressing,
    /// Disc ID computed from the rip's LOG/CUE artifacts — what we observed, not
    /// what editorial says, so it stays independent of the identified MB/Discogs
    /// row. A signal at re-identify time, and shown for confidence.
    pub disc_id: Option<String>,
    /// Where the metadata was seeded from. Distinct from identity:
    /// `metadata_source` answers "what should reset replay?", while
    /// the `release_identities` rows answer "which release(s) is this?".
    pub metadata_source: ReleaseMetadataSource,
    /// Specific MB/Discogs release ID used to seed metadata. NULL when
    /// `metadata_source = FileTags`.
    pub metadata_source_release_id: Option<String>,
    /// Shared, synced fact (the coven gate column): is this release's audio in
    /// the cloud home (remote) or local to one device (local). A local release's
    /// in-place files are registered with coven as the user's own external
    /// files; a remote release's bytes live in coven's blob cache.
    pub remote: bool,
    /// Name of the source folder this release was imported from (just the final
    /// path component, not the full path). Used to detect likely duplicates when
    /// the user re-scans the same folder.
    pub source_folder_name: Option<String>,
    /// SHA-256 over the imported folder's categorized file structure (sorted
    /// relative paths + sizes) — a location-independent content fingerprint.
    /// `None` for releases not created by a folder import. Metadata edits and
    /// re-identify preserve it (the files didn't change). Used to recognize an
    /// already-imported folder and to pick the overwrite target on re-import.
    pub content_hash: Option<String>,
    /// Album-level integrated loudness (EBU R128) in LUFS, measured at import
    /// over all tracks combined. `None` = not measured. Playback derives a gain
    /// from this against a constant target; the raw measurement is stored, never
    /// a gain.
    pub album_loudness_lufs: Option<f64>,
    /// Album-level true peak as a linear ratio (1.0 = 0 dBTP), the max across
    /// tracks. `None` = not measured. Playback caps the album gain at `1.0/peak`.
    pub album_peak_linear: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// The playing context of a saved `playback_state` row: the recipe to refill the
/// lane on restart — what it played from, and whether it was shuffled. A
/// substruct so "no context playing" (a single track, or nothing) is one `None`
/// instead of two columns that are only ever both-present or both-absent — see
/// `many-fields-none-together-means-a-missing-type`. The SQLite columns stay flat
/// (`source`, `shuffled`); the DB client splits this apart on save and
/// reassembles it on load.
#[derive(Debug, Clone, PartialEq)]
pub struct DbPlaybackContext {
    /// What the context plays from, encoded for the flat column: a release id, a
    /// JSON array of release ids, or the library sentinel. See
    /// `source_to_str`/`source_from_str` in `playback::persisted`.
    pub source: String,
    /// Whether the lane was shuffled. Restore permutes the refilled lane afresh
    /// — the session's shuffled order is deliberately not persisted.
    pub shuffled: bool,
}

/// The single device-local `playback_state` row. Mirrors the table columns; the
/// playback service maps this to and from its queue snapshot + live
/// position/volume/mute.
#[derive(Debug, Clone)]
pub struct DbPlaybackState {
    /// The playing context, or `None` for a single track / nothing playing.
    pub context: Option<DbPlaybackContext>,
    pub manual: String,
    pub repeat: String,
    pub current_track_id: Option<String>,
    pub position_ms: Option<i64>,
    pub volume: f32,
    pub is_muted: bool,
}

/// The outcome of reading the device-local `playback_state` resume cache. The
/// row is bae's own write, so a structurally-impossible row (a `source` present
/// without a `shuffled`, or vice versa) is corruption — kept distinct from an
/// absent row so the caller can count it and clear it rather than silently
/// starting fresh over a masked failure.
pub enum LoadedPlaybackState {
    Absent,
    Corrupt,
    Present(DbPlaybackState),
}

/// What a caller supplies to record one candidate's identify verdict via
/// [`crate::db::Database::save_import_candidate_verdict`] — the identify
/// columns of `import_candidate_state` except `identified_at`. That column is
/// stamped by the write path from the injected clock, the same convention as
/// `created_at` in `db/client/identity.rs`/`release.rs`: a timestamp that
/// records "when this write happened" is the DB layer's to assign, not data a
/// caller hands in — carrying it here would let a caller lie about it, and
/// would mean the sweep reaching for the ambient wall clock instead of the
/// fake-able one already threaded through `Database`.
///
/// It carries no file decisions: those are the user's half of the row and the
/// verdict write leaves them alone.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub struct NewImportCandidateVerdict {
    /// `CategorizedFiles::content_hash` — the row's identity. Adding,
    /// removing, or resizing a file changes this, which orphans the old row
    /// rather than updating it.
    pub content_hash: String,
    /// Where the candidate was last seen on disk. Not identity — the hash is —
    /// so a moved folder keeps reading the same row under its unchanged hash.
    pub folder_path: String,
    pub verdict: crate::identify::TerminalVerdict,
    /// Sum of the probed durations of the candidate's audio files, in
    /// milliseconds.
    pub probed_total_duration_ms: u64,
    /// File-decision revision used to derive this verdict.
    pub expected_edit_revision: u64,
    /// The identity the verdict itself decides — a single settled match IS the
    /// pick, made by identification instead of by a click. `None` decides
    /// nothing (several matches, a conflict, nothing found).
    ///
    /// Either way it replaces whatever identification concluded last time: the
    /// pick belongs to the verdict that made it. A pick a person made outranks
    /// both and is left alone.
    pub identity_pick: Option<crate::import::IdentityPick>,
}

/// What identification concluded about one candidate. Present as a whole or
/// absent as a whole: the identify columns and the match rows below them are
/// written together and cleared together, so no reader has to reason about a
/// half-filled result.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct DbCandidateIdentifyResult {
    pub verdict: crate::identify::TerminalVerdict,
    pub probed_total_duration_ms: u64,
    pub identified_at: DateTime<Utc>,
}

/// One loaded `import_candidate_state` row, as
/// [`crate::db::Database::load_import_candidate_states`] returns it. Mirrors
/// the table: one key, and the two independent things derived under it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq)]
pub struct DbImportCandidateState {
    pub content_hash: String,
    pub folder_path: String,
    /// What identification concluded, or `None` when nothing has identified
    /// this candidate yet — including when a file decision cleared what had,
    /// because that verdict described a folder shape that no longer applies.
    pub identify: Option<DbCandidateIdentifyResult>,
    /// The user's decisions about this candidate's files: which audio each
    /// track sheet describes, and which files are the release's tracks.
    pub file_edits: crate::import::folder_scanner::CandidateFileEdits,
    /// The identity decided for this candidate, or `None` while nothing is
    /// decided. A person's choice survives file decisions and later verdicts
    /// alike — it names a release, not a shape; one identification concluded
    /// lives exactly as long as the verdict that concluded it.
    pub identity_pick: Option<crate::import::IdentityPick>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub struct DbFolderScanSnapshot {
    pub watched_folder_path: String,
    pub generation: u64,
    pub status: crate::import::FolderScanStatus,
    pub items: Vec<crate::import::folder_scanner::ScanItem>,
}

/// Where `releases.metadata_source` came from.
///
/// Mirrors the `metadata_source` text column. Distinct from
/// `crate::import::MetadataSource` because that one only spans the two
/// editorial sources — this one also covers file-tags-only imports
/// (Unknown identity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseMetadataSource {
    MusicBrainz,
    Discogs,
    FileTags,
}

impl ReleaseMetadataSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::Discogs => "discogs",
            Self::FileTags => "file_tags",
        }
    }
}

impl std::str::FromStr for ReleaseMetadataSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "musicbrainz" => Ok(Self::MusicBrainz),
            "discogs" => Ok(Self::Discogs),
            "file_tags" => Ok(Self::FileTags),
            other => Err(format!("unknown release metadata source: {other}")),
        }
    }
}

/// A single track on a specific release, not on the logical album — track
/// listings vary between releases. Track artists are linked through the
/// `track_artists` junction table, so a track can credit features and artists
/// other than the album's.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrack {
    pub id: String,
    pub release_id: String,
    pub title: String,
    /// Physical side, 1-indexed and always set explicitly, never defaulted: a CD
    /// disc is side 1; vinyl A = 1, B = 2; a cassette has 2 sides.
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Position from metadata source (e.g., "A1", "1", "1-1")
    pub discogs_position: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// A physical file belonging to a release — not to an album, and not to a track
/// (some files are metadata like cover.jpg or .cue sheets that no track owns).
/// Holds enough of the original file's information to reconstruct the folder
/// structure on export.
///
/// Where the bytes live follows the release's storage state, resolved by coven:
/// a local release keeps the user's own file in place, registered with coven as
/// an external file and read straight from the user's path; a remote
/// release's bytes sit in coven's blob cache (`storage/pinned/` or
/// `storage/cache/`), read by file id through coven's locality-aware read — never
/// a bae path.
#[derive(Debug, Clone, PartialEq)]
pub struct DbFile {
    pub id: String,
    pub release_id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub content_type: ContentType,
    /// Readable cloud key for this file's blob, relative to the `release_files`
    /// namespace coven prepends (mirroring coven's `BlobRef.cloud_path`). `None`
    /// = the hashed-by-id layout (opaque homes); `Some` = the explicit readable
    /// key computed at import for a browsable home. coven derives every
    /// upload/read/delete key from this column via the table's `BlobDecl`.
    pub cloud_path: Option<String>,
    /// SHA-256 (lowercase hex) of this file's plaintext bytes — coven's
    /// author-signed content hash, verified against the decrypted bytes on
    /// every cloud fetch (see [`crate::util::fs::hash_file`]). Not optional:
    /// coven reads it off every blob-bearing row and refuses one without it, so
    /// a file row and its hash are written together or not at all.
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Track-level audio format metadata, one row per track: codec/display metadata,
/// pregap durations, and measured loudness. The file windows that supply the
/// track's samples live in `DbAudioSegment`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAudioFormat {
    pub id: String,
    pub track_id: String,
    pub content_type: ContentType,
    /// Pre-gap duration in milliseconds (CUE tracks with INDEX 00). When present,
    /// playback starts at INDEX 00 and shows negative time until INDEX 01.
    pub pregap_ms: Option<i64>,
    /// Generated silence before INDEX 01 from a CUE `PREGAP` directive. This is
    /// not bytes in the source file: natural playback emits zero samples for
    /// this duration before decoding the source audio; direct starts skip it.
    pub generated_pregap_ms: Option<i64>,
    /// Exact audio pregap length in source samples. Millisecond fields are for
    /// display/progress; export uses sample counts to place CUE gaps.
    pub pregap_samples: Option<i64>,
    /// Exact generated-silence pregap length in source samples.
    pub generated_pregap_samples: Option<i64>,
    /// Sample rate in Hz (for time-to-sample conversion during seek).
    pub sample_rate: i64,
    /// Bits per sample (16, 24, etc.). None for lossy codecs where FFmpeg can't determine it.
    pub bits_per_sample: Option<i64>,
    pub channels: i64,
    /// Per-track integrated loudness (EBU R128) in LUFS, measured at import over
    /// this track's sample window. `None` = not measured (decode/measure failure
    /// or a near-silent track with no usable loudness). Playback derives a gain
    /// from this against a constant target; the raw measurement is stored.
    pub track_loudness_lufs: Option<f64>,
    /// Per-track true peak as a linear ratio (1.0 = 0 dBTP), the max across
    /// channels. `None` = not measured. Playback caps the track gain at
    /// `1.0/peak`.
    pub track_peak_linear: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbAudioSegmentRole {
    AudioPregap,
    Main,
}

impl DbAudioSegmentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AudioPregap => "audio_pregap",
            Self::Main => "main",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "audio_pregap" => Some(Self::AudioPregap),
            "main" => Some(Self::Main),
            _ => None,
        }
    }
}

/// Where a segment begins and ends inside its backing file, in samples and in
/// bytes — the one declaration of the window every layer above the row carries
/// (resolved, prepared-for-playback, decode params). `end_sample`/`end_byte`
/// `None` = to end of file; `start_byte` `None` = byte 0 / no recorded landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentSpan {
    /// First sample of the segment within its backing file.
    pub start_sample: u64,
    /// One past the segment's last sample within its backing file.
    pub end_sample: Option<u64>,
    /// Byte the segment begins at within its backing file (the demuxer's
    /// recorded seek landing).
    pub start_byte: Option<u64>,
    /// One past the segment's last byte within its backing file.
    pub end_byte: Option<u64>,
}

impl SegmentSpan {
    /// The whole backing file: start at sample 0, run to EOF.
    pub fn whole_file() -> Self {
        Self {
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
        }
    }
}

/// One ordered file-backed window that supplies samples for an audio format.
/// The window columns stay `i64` (SQLite's integer type); [`Self::span`] is the
/// single conversion to the unsigned [`SegmentSpan`] everything above reads.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAudioSegment {
    pub id: String,
    pub audio_format_id: String,
    pub segment_index: i64,
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    /// First sample of this segment within its backing file.
    pub start_sample: i64,
    /// One past this segment's last sample within its backing file.
    pub end_sample: Option<i64>,
    /// Byte this segment begins at within its backing file. `None` means byte 0.
    pub start_byte: Option<i64>,
    /// One past this segment's last byte within its backing file.
    pub end_byte: Option<i64>,
    pub created_at: DateTime<Utc>,
}

impl DbAudioSegment {
    /// The row's window in unsigned coordinates. A negative stored value is a
    /// corrupt row; fail loud rather than wrap.
    pub fn span(&self) -> SegmentSpan {
        let non_negative = |what: &str, value: i64| {
            u64::try_from(value).unwrap_or_else(|_| {
                panic!("audio segment {} has negative {what}: {value}", self.id)
            })
        };
        SegmentSpan {
            start_sample: non_negative("start_sample", self.start_sample),
            end_sample: self.end_sample.map(|v| non_negative("end_sample", v)),
            start_byte: self.start_byte.map(|v| non_negative("start_byte", v)),
            end_byte: self.end_byte.map(|v| non_negative("end_byte", v)),
        }
    }
}
impl DbAlbumArtist {
    pub fn new(
        album_id: &str,
        artist_id: &str,
        position: i32,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbAlbumArtist {
            id,
            album_id: album_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            created_at: now,
        }
    }
}
impl DbTrackArtist {
    pub fn new(
        track_id: &str,
        artist_id: &str,
        position: i32,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrackArtist {
            id,
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            created_at: now,
        }
    }
}

impl DbWorkArtist {
    pub fn new(
        work_id: &str,
        artist_id: &str,
        position: i32,
        source: MetadataSource,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbWorkArtist {
            id,
            work_id: work_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbWorkPart {
    pub fn new(
        parent_work_id: &str,
        child_work_id: &str,
        position: i32,
        source: MetadataSource,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbWorkPart {
            id,
            parent_work_id: parent_work_id.to_string(),
            child_work_id: child_work_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbTrackWork {
    pub fn new(
        track_id: &str,
        work_id: &str,
        position: i32,
        source: MetadataSource,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrackWork {
            id,
            track_id: track_id.to_string(),
            work_id: work_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbReleaseArtistRole {
    pub fn new(
        release_id: &str,
        artist_id: &str,
        position: i32,
        source: MetadataSource,
        source_credit: Option<String>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbReleaseArtistRole {
            id,
            release_id: release_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            source_credit,
            created_at: now,
        }
    }
}

impl DbTrackArtistRole {
    pub fn new(
        track_id: &str,
        artist_id: &str,
        position: i32,
        source: MetadataSource,
        source_credit: Option<String>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrackArtistRole {
            id,
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            source_credit,
            created_at: now,
        }
    }
}
impl DbAlbum {
    #[cfg(test)]
    pub fn new_test(title: &str, artist_id: &str) -> Self {
        let now = chrono::Utc::now();
        DbAlbum {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            artist_id: artist_id.to_string(),
            year: None,
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn is_various_artists(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "various" || lower == "various artists"
}

impl DbRelease {
    #[cfg(test)]
    /// A minimal release fixture. It lands **Local**, the way every import does:
    /// a Remote release is one whose every blob reached the cloud, which no bare
    /// row insert can make true — coven refuses to register an external file
    /// against a Remote-gated row, and refuses to tombstone a blob that has no
    /// cloud object. A test that wants Remote takes a release through the real
    /// transition.
    pub fn new_test(album_id: &str, release_id: &str) -> Self {
        let now = chrono::Utc::now();
        DbRelease {
            id: release_id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            pressing: Pressing::blank(),
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        }
    }
    /// Storage state derived from the shared `remote` fact. Pinned-ness is an
    /// orthogonal coven-cache property the caller carries separately; it is never
    /// part of this.
    pub fn storage_state(&self) -> crate::album_detail::ReleaseStorageState {
        crate::album_detail::storage_state(self.remote)
    }
}

impl DbTrack {
    #[cfg(test)]
    pub fn new_test(
        release_id: &str,
        track_id: &str,
        title: &str,
        track_number: Option<i32>,
    ) -> Self {
        let now = chrono::Utc::now();
        DbTrack {
            id: track_id.to_string(),
            release_id: release_id.to_string(),
            title: title.to_string(),
            side: 1,
            track_number,
            duration_ms: None,
            discogs_position: None,

            created_at: now,
        }
    }
}
impl DbFile {
    /// Create a file record
    ///
    /// Files are linked to releases. Used for reconstructing original file structure
    /// during export. `content_hash` is coven's required blob content hash
    /// (see [`crate::util::fs::hash_file`]).
    pub fn new(
        release_id: &str,
        original_filename: &str,
        file_size: i64,
        content_type: ContentType,
        id: String,
        now: DateTime<Utc>,
        content_hash: String,
    ) -> Self {
        DbFile {
            id,
            release_id: release_id.to_string(),
            original_filename: original_filename.to_string(),
            file_size,
            content_type,
            // Files start opaque-keyed (hashed by id). A browsable remote
            // import / manage sets the readable key explicitly before insert.
            cloud_path: None,
            content_hash,
            created_at: now,
        }
    }
}
impl DbAudioFormat {
    pub fn new(
        track_id: &str,
        content_type: ContentType,
        sample_rate: i64,
        bits_per_sample: Option<i64>,
        channels: i64,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbAudioFormat {
            id,
            track_id: track_id.to_string(),
            content_type,
            pregap_ms: None,
            generated_pregap_ms: None,
            pregap_samples: None,
            generated_pregap_samples: None,
            sample_rate,
            bits_per_sample,
            channels,
            track_loudness_lufs: None,
            track_peak_linear: None,
            created_at: now,
        }
    }

    pub fn with_pregap(mut self, pregap_ms: Option<i64>) -> Self {
        self.pregap_ms = pregap_ms;
        self
    }

    pub fn with_generated_pregap(mut self, pregap_ms: Option<i64>) -> Self {
        self.generated_pregap_ms = pregap_ms;
        self
    }

    pub fn with_pregap_samples(mut self, pregap_samples: Option<i64>) -> Self {
        self.pregap_samples = pregap_samples;
        self
    }

    pub fn with_generated_pregap_samples(mut self, pregap_samples: Option<i64>) -> Self {
        self.generated_pregap_samples = pregap_samples;
        self
    }
}

/// One archived provider document, keyed by the source entity it describes.
/// Written before the verdict that names the release and read back after it
/// commits, so fields we don't map today can be extracted later without
/// re-fetching.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSourceReleasePayload {
    pub source: crate::import::PayloadSource,
    pub source_release_id: String,
    pub json: String,
    pub fetched_at: DateTime<Utc>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl DbSourceReleasePayload {
    pub fn new(payload: &crate::import::SourcePayload, now: DateTime<Utc>) -> Self {
        DbSourceReleasePayload {
            source: payload.source,
            source_release_id: payload.source_release_id.clone(),
            json: payload.json.clone(),
            fetched_at: now,
        }
    }
}
