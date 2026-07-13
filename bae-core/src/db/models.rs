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
    pub id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
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
/// implicitly when an album's releases hold rows in several sources. See
/// `notes/17-release-identity.md` for the loose attach rule and why album-level
/// identity columns don't fit it.
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
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
    /// in-place files are coven user-provided external refs (`local_blob_refs`);
    /// a remote release's bytes live in coven's blob cache.
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

/// The playing context of a saved `playback_state` row: what is being played, how
/// its tracks are ordered, and where the cursor sits. A substruct so "no context
/// playing" (a single track, or nothing) is one `None` instead of three columns
/// that are only ever all-present or all-absent — see
/// `many-fields-none-together-means-a-missing-type`. The SQLite columns stay flat
/// (`source`, `shuffle_seed`, `cursor`); the DB client splits this apart on save
/// and reassembles it on load.
#[derive(Debug, Clone, PartialEq)]
pub struct DbPlaybackContext {
    /// What the context plays from, encoded for the flat column: a release id, or
    /// the library sentinel. See `source_to_str`/`source_from_str` in
    /// `playback::persisted`.
    pub source: String,
    /// The `u64` shuffle seed reinterpreted as `i64` (SQLite's integer type) so
    /// the high bit round-trips. `None` = sequential (source) order.
    pub shuffle_seed: Option<i64>,
    /// Index into the (ordered) tracks of the track currently playing.
    pub cursor: i64,
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
/// without a `cursor`, or vice versa) is corruption — kept distinct from an
/// absent row so the caller can count it and clear it rather than silently
/// starting fresh over a masked failure.
pub enum LoadedPlaybackState {
    Absent,
    Corrupt,
    Present(DbPlaybackState),
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
/// a local release keeps the user's own file in place as a coven user-provided
/// external ref (`local_blob_refs`), read straight from the user's path; a remote
/// release's bytes sit in coven's blob cache (`storage/pinned/` or
/// `storage/cache/`), read by file id through coven's locality-aware read — never
/// a bae path.
#[derive(Debug, Clone)]
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
    /// every cloud fetch (see [`crate::util::fs::hash_file`]). `None` only for
    /// a row that predates the column (coven refuses to verify — and so
    /// refuses to fetch — an unhashed blob); every row this app writes now
    /// carries one.
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Track-level audio format metadata, one row per track: codec/display metadata,
/// pregap durations, and measured loudness. The file windows that supply the
/// track's samples live in `DbAudioSegment`.
#[derive(Debug, Clone)]
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

/// One ordered file-backed window that supplies samples for an audio format.
#[derive(Debug, Clone)]
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

impl DbWork {
    pub fn new(
        work_id: &str,
        title: &str,
        disambiguation: Option<String>,
        work_type: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        DbWork {
            id: work_id.to_string(),
            title: title.to_string(),
            disambiguation,
            work_type,
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

pub(crate) fn is_various_artists(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "various" || lower == "various artists"
}

impl DbRelease {
    #[cfg(test)]
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
            remote: true,
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
    /// (see [`crate::util::fs::hash_file`]) — `None` only when the caller has
    /// no real plaintext to hash against (a metadata-only test fixture that
    /// never exercises a real cloud fetch of this file's blob).
    pub fn new(
        release_id: &str,
        original_filename: &str,
        file_size: i64,
        content_type: ContentType,
        id: String,
        now: DateTime<Utc>,
        content_hash: Option<String>,
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

/// The full raw API response JSON, archived per source per release, so fields we
/// don't map today can be extracted later without re-fetching.
#[derive(Debug, Clone)]
pub struct DbReleaseMetadata {
    pub id: String,
    pub release_id: String,
    pub source: String,
    pub json: String,
    pub fetched_at: DateTime<Utc>,
}

impl DbReleaseMetadata {
    pub fn new(
        release_id: &str,
        source: &str,
        json: String,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbReleaseMetadata {
            id,
            release_id: release_id.to_string(),
            source: source.to_string(),
            json,
            fetched_at: now,
        }
    }
}

const IMPORT_OP_STATUS_IMPORTING: &str = "importing";
const IMPORT_OP_STATUS_COMPLETE: &str = "complete";
const IMPORT_OP_STATUS_FAILED: &str = "failed";
/// Status of an `imports` row. All validation happens before the import record is
/// created, so an import starts at Importing — there is no Preparing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportOperationStatus {
    Importing,
    Complete,
    Failed,
}
impl ImportOperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportOperationStatus::Importing => IMPORT_OP_STATUS_IMPORTING,
            ImportOperationStatus::Complete => IMPORT_OP_STATUS_COMPLETE,
            ImportOperationStatus::Failed => IMPORT_OP_STATUS_FAILED,
        }
    }
}
/// An import operation, from button click through completion. Created before any
/// other DB record exists, so progress subscriptions have a stable ID during
/// phase 0; `album_title` / `artist_name` are carried here for display until the
/// release they name exists.
#[derive(Debug, Clone)]
pub struct DbImport {
    pub id: String,
    pub status: ImportOperationStatus,
    /// Linked once phase 0 creates the release.
    pub release_id: Option<String>,
    pub album_title: String,
    pub artist_name: String,
    pub folder_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Set only when status is Failed.
    pub error_message: Option<String>,
}
impl DbImport {
    pub fn new(
        id: &str,
        album_title: &str,
        artist_name: &str,
        folder_path: &str,
        now: DateTime<Utc>,
    ) -> Self {
        let now = now.timestamp();
        DbImport {
            id: id.to_string(),
            status: ImportOperationStatus::Importing,
            release_id: None,
            album_title: album_title.to_string(),
            artist_name: artist_name.to_string(),
            folder_path: folder_path.to_string(),
            created_at: now,
            updated_at: now,
            error_message: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryImageType {
    Cover,
    Artist,
}

impl LibraryImageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LibraryImageType::Cover => "cover",
            LibraryImageType::Artist => "artist",
        }
    }

    pub fn namespace(&self) -> &'static str {
        match self {
            LibraryImageType::Cover => crate::sync::COVERS_NAMESPACE,
            LibraryImageType::Artist => crate::sync::ARTIST_IMAGES_NAMESPACE,
        }
    }
}

impl std::str::FromStr for LibraryImageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cover" => Ok(LibraryImageType::Cover),
            "artist" => Ok(LibraryImageType::Artist),
            other => Err(format!("Unknown library image type: {}", other)),
        }
    }
}

/// A cover or artist image. The bytes are a coven host-provided blob, in the
/// `covers` or `artist_images` namespace (per `image_type`), addressed by
/// `blob_id`.
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
    /// The id of the coven blob holding this image's bytes — distinct from `id`,
    /// which names the subject (the release or artist) and never moves. A coven
    /// blob id names one immutable byte-string, so each stored image gets a fresh
    /// `blob_id`: replacing a cover repoints the row at a new blob and deletes the
    /// old one, rather than writing new bytes under a live id.
    pub blob_id: String,
    pub image_type: LibraryImageType,
    pub content_type: ContentType,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// "local", "musicbrainz", "discogs"
    pub source: String,
    /// MB: CAA image ID, Discogs: URL, local: "release://{path}"
    pub source_url: Option<String>,
    /// Cloud object key for this image's blob, relative to the namespace coven
    /// prepends (`covers` / `artist_images`), mirroring coven's
    /// `BlobRef.cloud_path`. `None` = the hashed-by-id layout used by opaque
    /// homes; `Some` = the readable key set when the image entered a browsable
    /// home (cover: `{album_id}/{release_id}/cover.{ext}`, artist:
    /// `{artist_id}/artist.{ext}`). Only the cloud key becomes readable — coven's
    /// local cache layout is unaffected.
    pub cloud_path: Option<String>,
    /// SHA-256 (lowercase hex) of this image's plaintext bytes — coven's
    /// author-signed content hash (see [`crate::util::fs::hash_bytes`]).
    /// `None` only for a row that predates the column.
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl DbLibraryImage {
    /// A release's cover row, describing the bytes that will be stored as its
    /// blob. `bytes` is [`crate::util::cover::resize_cover`]'s output — the
    /// thumbnail itself, never the image it was made from — so `file_size`,
    /// `content_hash` and `content_type` are all derived from it here and cannot
    /// disagree with the blob. coven verifies the blob against `content_hash` on
    /// every cloud fetch, so a hash of any other bytes makes the cover
    /// unreadable on every other device.
    ///
    /// `content_type` is JPEG because the resize only ever emits JPEG.
    /// `cloud_path` is left `None`: it depends on the home's storage mode, and is
    /// set by whoever writes the row (the finalize transaction at import,
    /// `change_cover` from the row's own content type).
    ///
    /// `blob_id` is the id of the blob these bytes become, minted fresh by the
    /// caller for every stored image — a coven blob id names one immutable
    /// byte-string, so a cover that changes becomes a new blob rather than new
    /// bytes under the old id.
    pub fn cover(
        release_id: &str,
        blob_id: &str,
        source: &str,
        source_url: Option<String>,
        bytes: &[u8],
        now: DateTime<Utc>,
    ) -> Self {
        DbLibraryImage {
            id: release_id.to_string(),
            blob_id: blob_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type: ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: source.to_string(),
            source_url,
            cloud_path: None,
            content_hash: Some(crate::util::fs::hash_bytes(bytes)),
            created_at: now,
        }
    }
}

/// Raw combined search-result aggregate. No formatting — the resolver in
/// `LibraryManager` produces the display-ready `crate::album_detail::SearchResults`.
#[derive(Debug, Clone)]
pub struct DbLibrarySearchResults {
    pub albums: Vec<DbAlbumSearchResult>,
    pub tracks: Vec<DbTrackSearchResult>,
    pub composers: Vec<DbComposerSummary>,
    pub works: Vec<DbWorkSummary>,
}

/// Raw album search-result row with the primary artist name joined in SQL.
#[derive(Debug, Clone)]
pub struct DbAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub primary_release_id: Option<String>,
    pub artist_name: String,
}

/// Raw track search-result row with album and artist info joined in SQL.
/// No formatted duration label — the resolver in `LibraryManager` formats it.
#[derive(Debug, Clone)]
pub struct DbTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

#[derive(Debug, Clone)]
pub struct DbComposerSummary {
    pub artist: DbArtist,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
}

/// Raw artist-summary aggregate: the artist row plus its distinct album
/// count over both album-artist links (the primary `albums.artist_id` FK
/// and `album_artists` junction rows).
#[derive(Debug, Clone)]
pub struct DbArtistSummary {
    pub artist: DbArtist,
    pub album_count: i64,
}

/// Raw artist-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ArtistDetail`.
#[derive(Debug, Clone)]
pub struct DbArtistDetail {
    pub artist: DbArtistSummary,
    pub albums: Vec<DbAlbumSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkSummary {
    pub work: DbWork,
    pub parent_work_id: Option<String>,
    pub representative_release_id: Option<String>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
}

#[derive(Debug, Clone)]
pub struct DbComposerDetail {
    pub composer: DbComposerSummary,
    pub work_groups: Vec<DbComposerWorkGroup>,
    pub unlinked_release_roles: Vec<DbReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<DbTrackRoleSummary>,
}

#[derive(Debug, Clone)]
pub struct DbComposerWorkGroup {
    pub id: String,
    pub parent: Option<DbWorkSummary>,
    pub works: Vec<DbWorkSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkDetail {
    pub work: DbWorkSummary,
    pub child_works: Vec<DbWorkSummary>,
    pub releases: Vec<DbWorkReleaseSummary>,
    pub tracks: Vec<DbWorkTrackSummary>,
}

#[derive(Debug, Clone)]
pub struct DbWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub release_index: i64,
}

#[derive(Debug, Clone)]
pub struct DbReleaseRoleSummary {
    pub role: DbReleaseArtistRole,
    pub album: DbAlbum,
}

#[derive(Debug, Clone)]
pub struct DbTrackRoleSummary {
    pub role: DbTrackArtistRole,
    pub track: DbTrack,
    pub album: DbAlbum,
    pub artist: DbArtist,
}

#[derive(Debug, Clone)]
pub struct DbWorkTrackSummary {
    pub link: DbTrackWork,
    pub track: DbTrack,
    pub album: DbAlbum,
}

/// Raw per-release storage summary, assembled in one SQL query (no N+1). No
/// formatting, no derivation — the resolver in `LibraryManager` produces the
/// display-ready `crate::album_detail::ReleaseStorageSummary` (deriving
/// `storage_state` from `remote` and formatting `total_size`). Pending-upload
/// counts are not here; `OutboxSnapshot` is the only source for those.
#[derive(Debug, Clone)]
pub struct DbReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub format: Option<String>,
    pub primary_release_id: Option<String>,
    /// The shared `releases.remote` fact: audio in the cloud vs local to a device.
    /// The resolver reads `Local` straight off `!remote`; for a remote release it
    /// asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has none. Used to
    /// ask coven's cache whether the release is pinned — pin/unpin act on all a
    /// release's blobs together, so any one file stands for the release.
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumSortCriterion {
    pub field: AlbumSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerSortField {
    Name,
    WorkCount,
    LinkedReleaseCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposerSortCriterion {
    pub field: ComposerSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistSortField {
    Name,
    AlbumCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtistSortCriterion {
    pub field: ArtistSortField,
    pub direction: SortDirection,
}

/// Raw album-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::AlbumDetail`.
#[derive(Debug, Clone)]
pub struct DbAlbumDetail {
    pub album: DbAlbum,
    pub artists: Vec<DbArtist>,
    pub releases: Vec<DbReleaseDetail>,
}

/// Raw release-detail aggregate. The resolver in `LibraryManager` produces
/// the display-ready `crate::album_detail::ReleaseDetail`.
#[derive(Debug, Clone)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    pub tracks: Vec<DbTrackWithArtists>,
    pub files: Vec<DbFile>,
    /// Audio-format rows for this release's tracks: codec, sample rate, bit
    /// depth, channels. The file-backed windows live in `audio_segments`.
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
    /// All identity rows for this release. Empty for Unknown imports.
    pub identities: Vec<crate::import::ReleaseIdentity>,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    pub artists: Vec<DbArtist>,
}

/// Raw per-release slim aggregate for summary views (storage rows, release
/// pickers): `DbReleaseStorageSummary` minus the album-level joins. The resolver
/// in `LibraryManager` produces the display-ready
/// `crate::album_detail::ReleaseSummary`. Pending-upload counts are not here;
/// `OutboxSnapshot` is the only source for those.
#[derive(Debug, Clone)]
pub struct DbReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// The shared `releases.remote` fact: audio in the cloud vs local to a device.
    /// The resolver reads `Local` straight off `!remote`; for a remote release it
    /// asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has none. Used to
    /// ask coven's cache whether the release is pinned — pin/unpin act on all a
    /// release's blobs together, so any one file stands for the release.
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

/// Raw storage-page row: a release summary joined with its parent album summary,
/// both halves from one SQL query. The resolver in `LibraryManager` turns them
/// into `ReleaseSummary` / `AlbumSummary`, which the UI normalizes into slices.
#[derive(Debug, Clone)]
pub struct DbStorageRow {
    pub release: DbReleaseSummary,
    pub album: DbAlbumSummary,
}

/// The columns the Storage Manager view renders, as sort keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

/// A single sort criterion for storage-page queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSortCriterion {
    pub field: StorageSortField,
    pub direction: SortDirection,
}

/// Filter applied to a storage-page query — the four mutually-exclusive chips
/// the Storage Manager shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    Remote,
    Local,
    Uploading,
}

/// The operations `Database::outbox_items` exposes from `cloud_outbox`.
/// coven's internal `cancel` rows are filtered out by that query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbOutboxOperation {
    Upload,
    Delete,
}

impl DbOutboxOperation {
    /// Parse the visible `cloud_outbox.operation` text column values.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "upload" => Some(Self::Upload),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// One row from the `cloud_outbox` join: the queue entry's own columns plus the
/// joined release id, album title, and file size, from which the snapshot builder
/// constructs grouped uploads and deletes.
///
/// Only an upload has a `file_id`; a delete names its blob by `cloud_key` alone.
/// `release_id`, `title`, `file_name`, and `file_size` are `Option` because the
/// `release_files` join finds nothing for a delete, or misses an orphaned
/// `file_id` (the row's file was deleted before the outbox drained).
#[derive(Debug, Clone)]
pub struct DbOutboxRow {
    pub id: i64,
    pub operation: DbOutboxOperation,
    pub file_id: Option<String>,
    pub cloud_key: String,
    /// Enqueue time as Unix epoch milliseconds, parsed from the queue row's
    /// RFC 3339 `created_at` column at read time so consumers carry an instant.
    pub created_at: i64,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub release_id: Option<String>,
    pub title: Option<String>,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::DbOutboxOperation;

    #[test]
    fn db_outbox_operation_parse_is_the_visible_domain() {
        assert_eq!(
            DbOutboxOperation::parse("upload"),
            Some(DbOutboxOperation::Upload)
        );
        assert_eq!(
            DbOutboxOperation::parse("delete"),
            Some(DbOutboxOperation::Delete)
        );
        assert_eq!(DbOutboxOperation::parse("cancel"), None);
        assert_eq!(DbOutboxOperation::parse("bogus"), None);
    }
}
