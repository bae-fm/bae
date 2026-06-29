//! Raw DB-shape types: table rows and the aggregates that SQL joins produce.
//!
//! Two flavors live here:
//! - `Db*` row types mirror a table row (`DbAlbum`, `DbArtist`, `DbRelease`,
//!   `DbTrack`, `DbFile`, etc.).
//! - `Db*Detail` / `Db*Summary` / `Db*SearchResult` aggregates are the shape
//!   of a join query's result: multiple rows stitched together in SQL, not
//!   in Rust.
//!
//! **No formatting, no Rust-side derivation, no filesystem access.** Raw
//! rows and raw aggregates only. Every `*_label`, sum-of-fields, group-by,
//! path resolution, `has_X` flag, or `stored.or_else(first)` fallback
//! belongs in the manager, not here.
//!
//! `LibraryManager` in `crate::library::manager` is the resolver: it takes
//! these raw types and produces the display-ready `AlbumDetail`,
//! `ReleaseDetail`, `AlbumSummary`, `ReleaseStorageSummary`, and
//! `SearchResults` that live in `crate::album_detail`. Those are what the
//! bridge and event payloads carry. (The queue's `crate::queue::QueueItem` is
//! built directly by `db::get_queue_items` — it has no raw counterpart here.)

use crate::import::MetadataSource;
use crate::util::content_type::ContentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Artist metadata
///
/// Represents an individual artist or band. Artists are linked to albums and tracks
/// via junction tables (album_artists, track_artists) to support:
/// - Multiple artists per album (collaborations)
/// - Different artists per track (compilations, features)
/// - Artist deduplication across imports
///
/// Supports multiple metadata sources:
/// - Discogs: discogs_artist_id for deduplication
/// - Other sources can be added as needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
/// Destructures all source ID fields so adding a new source without
/// updating this function causes a compile error.
pub struct VariousArtistsIds {
    pub discogs: &'static str,
    pub musicbrainz: &'static str,
}

pub const VARIOUS_ARTISTS: VariousArtistsIds = VariousArtistsIds {
    discogs: "194",
    musicbrainz: "89ad4ac3-39f7-470e-963a-56509c546377",
};

impl DbArtist {
    /// Returns true if this artist is the well-known "Various Artists" placeholder
    /// from any metadata source. Exhaustive over all source ID fields — adding a
    /// new `*_artist_id` field to DbArtist without updating this function will
    /// cause a compile error.
    pub fn is_various_artists(&self) -> bool {
        // Destructure to force a compile error when a new source ID field is added
        let DbArtist {
            discogs_artist_id,
            musicbrainz_artist_id,
            // Non-source fields — ignored
            id: _,
            name: _,
            sort_name: _,
            created_at: _,
        } = self;

        discogs_artist_id.as_deref() == Some(VARIOUS_ARTISTS.discogs)
            || musicbrainz_artist_id.as_deref() == Some(VARIOUS_ARTISTS.musicbrainz)
    }
}

/// Links artists to albums (many-to-many)
///
/// Supports albums with multiple artists (e.g., collaborations).
/// Position field maintains the order of artists for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAlbumArtist {
    pub id: String,
    pub album_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist albums (0-indexed)
    pub position: i32,
    pub created_at: DateTime<Utc>,
}
/// Links artists to tracks (many-to-many)
///
/// Supports tracks with multiple artists (features, remixes, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTrackArtist {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist tracks (0-indexed)
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbWork {
    pub id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbWorkArtist {
    pub id: String,
    pub work_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbWorkPart {
    pub id: String,
    pub parent_work_id: String,
    pub child_work_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbTrackWork {
    pub id: String,
    pub track_id: String,
    pub work_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbReleaseArtistRole {
    pub id: String,
    pub release_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbTrackArtistRole {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: MetadataSource,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Album metadata - represents a logical album (the "master")
///
/// A logical album can have multiple physical releases (e.g., "1973 Original", "2016 Remaster").
/// This table stores the high-level album information that's common across all releases.
/// Specific release details and import status are tracked in the `releases` table.
///
/// Artists are linked via the `album_artists` junction table to support multiple artists.
///
/// Albums carry no per-source identity of their own — identity lives on
/// the `release_identities` side table per release. Cross-source equivalences
/// surface implicitly when the album's releases hold rows in multiple sources.
/// See `notes/17-release-identity.md` for the design rationale (in particular,
/// the loose attach rule and why album-level identity columns don't fit it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
/// Raw album-summary aggregate: artist names, release IDs, and album
/// artist IDs joined in SQL (via `json_group_array`) to avoid N+1 lookups.
/// The resolver in `LibraryManager` produces the display-ready
/// `crate::album_detail::AlbumSummary` (applies the `primary_release_id`
/// fallback).
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

/// Probe whether the candidate at `release_id` (from a search result) is
/// already represented in the library, either as the same pressing
/// (source + source_release_id) or as the same album (any release with
/// the same source + source_group_id). Result is correlated back to the
/// candidate via `release_id`.
///
/// `source` + `source_group_id` come from the search result; the
/// candidate's group ID is what the picker shows. `Option<String>` on
/// `source_group_id` covers candidates where the search result didn't
/// surface a group (rare for MB, can happen for Discogs releases without
/// a master) — those checks skip the album-level lookup.
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

/// The pressing-level cluster of a release (the specific physical
/// pressing's editorial metadata). Held as a substruct so "no pressing
/// claim" is one assignment (`Pressing::blank()`) instead of nilling
/// six fields in every caller — see `many-fields-none-together-means-
/// a-missing-type`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pressing {
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    /// Format (e.g., "CD", "Vinyl", "Digital")
    pub format: Option<String>,
    /// Record label
    pub label: Option<String>,
    /// Catalog number
    pub catalog_number: Option<String>,
    /// Country of release
    pub country: Option<String>,
    /// Barcode
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

/// Release metadata - represents a specific version/pressing of an album
///
/// A release is a physical or digital version of a logical album.
/// Examples: "1973 Original Pressing", "2016 Remaster", "180g Vinyl", "Digital Release"
///
/// Files and tracks belong to releases (not albums), because:
/// - Users import specific releases, not abstract albums
/// - Each release has its own audio files and metadata
/// - Multiple releases of the same album can coexist in the library
///
/// The release_name field distinguishes between versions (e.g., "2016 Remaster").
/// If the user doesn't specify a release, we create one with release_name=None.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbRelease {
    pub id: String,
    /// Links to the logical album (DbAlbum)
    pub album_id: String,
    /// Human-readable release name (e.g., "2016 Remaster", "180g Vinyl")
    pub release_name: Option<String>,
    /// Pressing-level editorial metadata. `Pressing::blank()` means
    /// "user claimed an album, not a specific pressing" (Approximate
    /// imports). The DB columns stay flat (`year`, `format`, `label`,
    /// …) — this grouping is Rust-side only.
    pub pressing: Pressing,
    /// Disc ID computed from the rip's LOG/CUE artifacts. Independent of
    /// the identified MB/Discogs row — this is what we observed, not what
    /// editorial says. Used as a signal at re-identify time and rendered
    /// in the compact line for confidence.
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
    /// `None` for releases not created by a folder import. The import worker
    /// populates it just before insert; metadata edits / re-identify preserve
    /// it (the files didn't change). Used to recognize an already-imported
    /// folder and to pick the overwrite target on re-import.
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

/// The playing context of a saved `playback_state` row: which release is being
/// played, how its tracks are ordered, and where the cursor sits. Held as a
/// substruct so "no context playing" (a single track, or nothing) is one
/// `None` instead of three separately-nullable columns that are only ever
/// present or absent together — see `many-fields-none-together-means-a-missing-
/// type`. The SQLite columns stay flat (`source`, `shuffle_seed`, `cursor`);
/// the DB client destructures this on save and reassembles it on load.
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

/// Where `releases.metadata_source` came from.
///
/// Mirrors the `metadata_source` text column. Distinct from
/// `crate::import::MetadataSource` because that one only spans the two
/// editorial sources — this one also covers file-tags-only imports
/// (Unknown identity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Track metadata within a release
///
/// Represents a single track on a specific release. Tracks are linked to releases
/// (not logical albums) because track listings can vary between releases.
///
/// Track artists are linked via the `track_artists` junction table to support:
/// - Multiple artists per track (features, collaborations)
/// - Different artists than the album artist (compilations)
///
/// The discogs_position field stores the track position from metadata
/// (e.g., "A1", "1", "1-1" for vinyl sides).
///
/// `side` is the physical side (1-indexed). A CD = 1 side. Vinyl = 2 sides per disc.
/// Cassette = 2 sides. Always set explicitly, never defaulted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbTrack {
    pub id: String,
    /// Links to the specific release (DbRelease), not the logical album
    pub release_id: String,
    pub title: String,
    /// Physical side number (1-indexed). CD disc 1 = side 1. Vinyl A = side 1, B = side 2, etc.
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Position from metadata source (e.g., "A1", "1", "1-1")
    pub discogs_position: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Physical file belonging to a release
///
/// Stores original file information needed to reconstruct file structure for export.
///
/// Files are linked to releases (not logical albums or tracks), because:
/// - Files are part of a specific release (e.g., "2016 Remaster" has different files than "1973 Original")
/// - Some files are metadata (cover.jpg, .cue sheets) not associated with any track
///
/// File location follows the release's storage state, resolved by coven:
/// - Local: the user's own file in place, a coven user-provided external ref
///   (`local_blob_refs`), read straight from the user's path;
/// - Remote: coven's blob cache (`storage/pinned/` or `storage/cache/`), read
///   through coven's locality-aware read by the file's id — never a bae path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFile {
    pub id: String,
    /// Release this file belongs to
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
    pub created_at: DateTime<Utc>,
}

/// Audio format metadata for a track. One record per track (1:1 with track).
///
/// A track is a sample window `[start_sample, end_sample)` into its backing
/// file: standalone per-track files use `(0, None)` (the whole file), CUE tracks
/// carry the track's bounds. Playback decodes the file natively (FFmpeg) and
/// seeks / stops by sample -- there is no byte-range extraction or synthetic
/// header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAudioFormat {
    pub id: String,
    pub track_id: String,
    pub content_type: ContentType,
    /// Pre-gap duration in milliseconds (CUE tracks with INDEX 00). When present,
    /// playback starts at INDEX 00 and shows negative time until INDEX 01.
    pub pregap_ms: Option<i64>,
    /// Sample rate in Hz (for time-to-sample conversion during seek).
    pub sample_rate: i64,
    /// Bits per sample (16, 24, etc.). None for lossy codecs where FFmpeg can't determine it.
    pub bits_per_sample: Option<i64>,
    pub channels: i64,
    /// FK to DbFile containing this track's audio data.
    pub file_id: Option<String>,
    /// First sample of this track within its backing file (0 for a whole-file track).
    pub start_sample: i64,
    /// One past this track's last sample within its backing file (None = to EOF).
    pub end_sample: Option<i64>,
    /// One past this track's last byte within its backing file (None = to EOF /
    /// a whole-file track). Frame-granular, computed at import by seeking.
    /// Playback buffers the rest of the current track up to here, ahead of the
    /// playhead, rather than a fixed window.
    pub end_byte: Option<i64>,
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
    /// Create a logical album from a Discogs release
    /// Note: Artists should be created separately and linked via DbAlbumArtist
    ///
    /// master_year is the original release year (from the Discogs master release).
    /// Falls back to the specific release year if master_year is unavailable.
    pub fn from_discogs_release(
        release: &crate::discogs::DiscogsRelease,
        master_year: Option<u32>,
        primary_artist_id: &str,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        let is_compilation = release
            .artists
            .first()
            .map(|a| is_various_artists(&a.name))
            .unwrap_or(false);
        let year = master_year
            .map(|y| y as i32)
            .or(release.year.map(|y| y as i32));
        DbAlbum {
            id,
            title: release.title.clone(),
            artist_id: primary_artist_id.to_string(),
            year,
            primary_release_id: None,
            is_compilation,
            created_at: now,
        }
    }
    pub fn from_mb_response(
        response: &crate::musicbrainz::MbReleaseResponse,
        master_year: Option<u32>,
        primary_artist_id: &str,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        let first_release_date = response
            .release_group
            .as_ref()
            .and_then(|rg| rg.first_release_date.clone())
            .filter(|s| !s.is_empty());
        let year = first_release_date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()))
            .or_else(|| {
                response
                    .date
                    .as_ref()
                    .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()))
            })
            .or(master_year.map(|y| y as i32));
        let is_compilation = response
            .artist_credit
            .first()
            .map(|ac| is_various_artists(&ac.name))
            .unwrap_or(false);
        DbAlbum {
            id,
            title: response.title.clone(),
            artist_id: primary_artist_id.to_string(),
            year,
            primary_release_id: None,
            is_compilation,
            created_at: now,
        }
    }
}

/// Check if an artist name indicates a "Various Artists" compilation
fn is_various_artists(name: &str) -> bool {
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
    /// Create a release from a Discogs release
    pub fn from_discogs_release(
        album_id: &str,
        release: &crate::discogs::DiscogsRelease,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        let format = if release.format.is_empty() {
            None
        } else {
            Some(release.format.join(", "))
        };
        DbRelease {
            id,
            album_id: album_id.to_string(),
            release_name: None,
            pressing: Pressing {
                year: release.year.map(|y| y as i32),
                format,
                label: release.label.first().cloned(),
                catalog_number: release.catno.clone(),
                country: release.country.clone(),
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::Discogs,
            metadata_source_release_id: Some(release.id.clone()),
            // Imports land local; the upload observer flips `remote` true
            // once the release's audio is durably in the cloud.
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        }
    }
    pub fn from_mb_response(
        album_id: &str,
        response: &crate::musicbrainz::MbReleaseResponse,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        let year = response
            .date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()));
        let format = response.media.first().and_then(|m| m.format.clone());
        let (label, catalog_number) = response
            .label_info
            .first()
            .map(|li| {
                (
                    li.label.as_ref().and_then(|l| l.name.clone()),
                    li.catalog_number.clone(),
                )
            })
            .unwrap_or((None, None));
        DbRelease {
            id,
            album_id: album_id.to_string(),
            release_name: None,
            pressing: Pressing {
                year,
                format,
                label,
                catalog_number,
                country: response.country.clone(),
                barcode: response.barcode.clone(),
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::MusicBrainz,
            metadata_source_release_id: Some(response.id.clone()),
            // Imports land local; the upload observer flips `remote` true
            // once the release's audio is durably in the cloud.
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        }
    }

    /// Storage state — Local (local) or Remote (cloud) — from the shared
    /// `remote` fact. Pinned-ness is the orthogonal coven-cache property the
    /// caller carries separately; it is never part of this.
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
    pub fn from_discogs_track(
        title: &str,
        release_id: &str,
        track_number: i32,
        side: i32,
        discogs_position: Option<String>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrack {
            id,
            release_id: release_id.to_string(),
            title: title.to_string(),
            side,
            track_number: Some(track_number),
            duration_ms: None,
            discogs_position,

            created_at: now,
        }
    }
}
impl DbFile {
    /// Create a file record
    ///
    /// Files are linked to releases. Used for reconstructing original file structure
    /// during export.
    pub fn new(
        release_id: &str,
        original_filename: &str,
        file_size: i64,
        content_type: ContentType,
        id: String,
        now: DateTime<Utc>,
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
            created_at: now,
        }
    }
}
impl DbAudioFormat {
    /// Build an audio format for a track that spans `[start_sample, end_sample)`
    /// of its backing file. Use `(0, None)` for a whole-file (per-track) source.
    pub fn new(
        track_id: &str,
        content_type: ContentType,
        sample_rate: i64,
        bits_per_sample: Option<i64>,
        channels: i64,
        start_sample: i64,
        end_sample: Option<i64>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbAudioFormat {
            id,
            track_id: track_id.to_string(),
            content_type,
            pregap_ms: None,
            sample_rate,
            bits_per_sample,
            channels,
            file_id: None,
            start_sample,
            end_sample,
            end_byte: None,
            track_loudness_lufs: None,
            track_peak_linear: None,
            created_at: now,
        }
    }

    /// Set the file_id linking to DbFile.
    pub fn with_file_id(mut self, file_id: &str) -> Self {
        self.file_id = Some(file_id.to_string());
        self
    }

    /// Set the track's end byte within its backing file. The default `None` is
    /// the whole file -- correct for a per-track source; a single-file album's
    /// track sets its own end byte, computed at import.
    pub fn with_end_byte(mut self, end_byte: Option<i64>) -> Self {
        self.end_byte = end_byte;
        self
    }

    /// Set pregap duration (CUE tracks with INDEX 00).
    pub fn with_pregap(mut self, pregap_ms: Option<i64>) -> Self {
        self.pregap_ms = pregap_ms;
        self
    }
}

/// Raw API response JSON stored per source per release.
///
/// Used to archive the full API response so fields not currently mapped
/// can be extracted later without re-fetching.
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
/// Status of an import operation (tracked in the `imports` table).
///
/// All validation happens before the import record is created, so
/// imports start at Importing (no Preparing state needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// Tracks an import operation from button click through completion
///
/// Created when user clicks Import, before any database records exist.
/// Provides a stable ID for progress subscriptions during phase 0.
/// Linked to release_id after phase 0 completes and release is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbImport {
    pub id: String,
    pub status: ImportOperationStatus,
    /// Linked after phase 0 when release is created
    pub release_id: Option<String>,
    /// Album title for display before release exists
    pub album_title: String,
    /// Artist name for display
    pub artist_name: String,
    /// Source folder path
    pub folder_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Error message if status is Failed
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
/// Type discriminator for library images
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

/// bae-remote metadata image (cover art, artist photo).
/// File lives at a deterministic path derived from type + id:
/// - Cover: covers/{id}
/// - Artist: artists/{id}
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
    pub image_type: LibraryImageType,
    pub content_type: ContentType,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// "local", "musicbrainz", "discogs"
    pub source: String,
    /// MB: CAA image ID, Discogs: URL, local: "release://{path}"
    pub source_url: Option<String>,
    /// Cloud object key for this image's blob (relative to the `images`
    /// namespace coven prepends), mirroring coven's `BlobRef.cloud_path`.
    /// `None` = the hashed-by-id layout used by opaque homes; `Some` = the
    /// explicit readable key set when the image entered a browsable home
    /// (cover: `{artist}/{album}/cover.{ext}`, artist: `{artist}/artist.{ext}`).
    /// The local on-disk image file stays at the hashed `image_path(id)`
    /// regardless — only the cloud key becomes readable.
    pub cloud_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Library Search Result Types
// ============================================================================

/// Raw combined search-result aggregate across albums and tracks.
/// No formatting — the resolver in `LibraryManager` produces the
/// display-ready `crate::album_detail::SearchResults`.
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
    pub releases: Vec<DbReleaseSummary>,
    pub tracks: Vec<DbWorkTrackSummary>,
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

/// Raw per-release storage summary assembled via a single SQL query (no
/// N+1). No formatting, no derivation — the resolver in `LibraryManager`
/// produces the display-ready `crate::album_detail::ReleaseStorageSummary`
/// (derives `storage_state` from the two columns and formats `total_size`).
///
/// Pending-upload counts are no longer carried here; the `OutboxSnapshot`
/// is the single source of truth, reactive on every queue mutation.
#[derive(Debug, Clone)]
pub struct DbReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub format: Option<String>,
    pub primary_release_id: Option<String>,
    /// Shared `releases.remote` fact: remote (cloud) vs local (local). The
    /// resolver derives `Local` directly from `!remote`; for a remote
    /// release it asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has no files. Used
    /// to ask coven's cache whether the release is pinned (pin/unpin act on all a
    /// release's blobs together, so any one file represents the release).
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

/// Field to sort albums by
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A single sort criterion (field + direction)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumSortCriterion {
    pub field: AlbumSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Audio-format rows for this release's tracks. Each carries the codec,
    /// sample rate, bit depth, channels, and its `file_id`, so the resolver can
    /// attach a format descriptor to each audio file. A single-file CUE rip has
    /// many rows sharing one `file_id` (one per track).
    pub audio_formats: Vec<DbAudioFormat>,
    /// All identity rows for this release. Empty for Unknown imports.
    pub identities: Vec<crate::import::ReleaseIdentity>,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    pub artists: Vec<DbArtist>,
}

/// Raw per-release slim aggregate for summary views (storage rows,
/// release pickers). Same core shape as `DbReleaseStorageSummary` minus
/// the album-level joins. The resolver in `LibraryManager` produces the
/// display-ready `crate::album_detail::ReleaseSummary`.
///
/// Pending-upload counts are no longer carried here; the `OutboxSnapshot`
/// is the single source of truth, reactive on every queue mutation.
#[derive(Debug, Clone)]
pub struct DbReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// Shared `releases.remote` fact: remote (cloud) vs local (local). The
    /// resolver derives `Local` directly from `!remote`; for a remote
    /// release it asks coven's cache (via `any_file_id`) whether it is pinned.
    pub remote: bool,
    /// The id of one of the release's files, or `None` when it has no files. Used
    /// to ask coven's cache whether the release is pinned (pin/unpin act on all a
    /// release's blobs together, so any one file represents the release).
    pub any_file_id: Option<String>,
    pub file_count: i64,
    pub total_size: i64,
}

/// Raw storage-page row: release summary joined with its parent album
/// summary. One SQL query emits both halves; the resolver in
/// `LibraryManager` produces two pre-assembled `ReleaseSummary` /
/// `AlbumSummary` aggregates the UI normalizes into its slices.
#[derive(Debug, Clone)]
pub struct DbStorageRow {
    pub release: DbReleaseSummary,
    pub album: DbAlbumSummary,
}

/// Field to sort storage rows by. Mirrors the columns the Storage
/// Manager view renders today.
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

/// Filter applied to a storage-page query. Mirrors the four
/// mutually-exclusive chips the Storage Manager shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    Remote,
    Local,
    Uploading,
}

/// The full `cloud_outbox.operation` domain (matches the schema CHECK): the
/// three values coven can write to the queue. `Cancel` is a coven-internal
/// tombstone-retry row (a delete coven re-issues after an upload races it); it
/// renders nothing in the UI and is excluded from the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxOpKind {
    Upload,
    Delete,
    Cancel,
}

impl OutboxOpKind {
    /// Parse the `cloud_outbox.operation` text column. Total over the real
    /// domain — a value outside it is genuine corruption (`None`), not a kind we
    /// declined to model.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "upload" => Some(Self::Upload),
            "delete" => Some(Self::Delete),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// The outbox operations the UI snapshot displays — the `Cancel`-free subset of
/// `OutboxOpKind`. `outbox_items` maps each row into this, dropping coven's
/// internal `cancel` rows, so the snapshot builder can't represent (or forget to
/// handle) a `Cancel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayedOutboxOp {
    Upload,
    Delete,
}

/// One row from the `cloud_outbox` join: the queue entry's own columns plus
/// the joined release id, album title, and file size. The snapshot builder
/// uses these to construct `UploadOp` / `DeleteOp`.
///
/// `file_id` is `None` for a delete (it carries no file id — the blob is named
/// by `cloud_key`); only an upload has one. `release_id`, `title`, `file_name`,
/// and `file_size` are `Option` because the `release_files` join finds nothing
/// for a delete, or misses an orphaned `file_id` (the row's file was deleted
/// before the outbox drained).
#[derive(Debug, Clone)]
pub struct DbOutboxRow {
    pub id: i64,
    pub operation: DisplayedOutboxOp,
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
    use super::OutboxOpKind;

    #[test]
    fn outbox_op_kind_parse_is_total_over_the_domain() {
        assert_eq!(OutboxOpKind::parse("upload"), Some(OutboxOpKind::Upload));
        assert_eq!(OutboxOpKind::parse("delete"), Some(OutboxOpKind::Delete));
        assert_eq!(OutboxOpKind::parse("cancel"), Some(OutboxOpKind::Cancel));
        // A value outside the domain is genuine corruption, not an unmodeled kind.
        assert_eq!(OutboxOpKind::parse("bogus"), None);
    }
}
